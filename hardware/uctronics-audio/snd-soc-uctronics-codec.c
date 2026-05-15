// SPDX-License-Identifier: GPL-2.0
/*
 * Uctronics AI in a Box codec driver
 *
 * Wraps an I2S speaker amplifier (with sdmode + 3-bit gain select GPIOs)
 * and a MEMS digital microphone into a single ASoC codec / ALSA card.
 *
 * Reverse-engineered from the proprietary uctronics,uctronics-codec
 * driver in the Useful Sensors baseline image (kernel 5.10.110-102).
 * Disassembly: 7 text functions at 0xbda1c8-0xbda5d4.
 *
 * GPIO bindings (from device tree):
 *   sdmode-gpios     = GPIO3_B5  — speaker amp enable (active high)
 *   gainsel_1-gpios  = GPIO3_A3  — gain select bit (MSB)
 *   gainsel_2-gpios  = GPIO3_A5  — gain select mid
 *   gainsel_3-gpios  = GPIO3_A2  — gain select bit (LSB)
 *
 * Gain table (from disassembly jump table at 0xbda398-0xbda4d0):
 *   Vol 0: g1=0 g2=0 g3=0  (minimum, ~6 dB)
 *   Vol 1: g1=0 g2=0 g3=1  (~9 dB)
 *   Vol 2: g1=0 g2=1 g3=0  (~12 dB)
 *   Vol 3: g1=0 g2=1 g3=1  (~15 dB, default)
 *   Vol 4: g1=1 g2=0 g3=0  (~18 dB, maximum)
 *
 * Speaker amp (sdmode) is toggled in DAI startup/shutdown:
 *   - Playback start: amp ON
 *   - Playback stop:  amp OFF
 *   - Capture start:  amp OFF (prevent feedback)
 */

#include <linux/module.h>
#include <linux/platform_device.h>
#include <linux/of.h>
#include <linux/gpio/consumer.h>
#include <sound/soc.h>
#include <sound/pcm.h>

struct uc_codec_priv {
	struct gpio_desc *sdmode_gpio;    /* +0:  speaker amp enable */
	struct gpio_desc *gainsel_1_gpio; /* +8:  gain MSB */
	struct gpio_desc *gainsel_2_gpio; /* +16: gain mid */
	struct gpio_desc *gainsel_3_gpio; /* +24: gain LSB */
	int volume;                       /* +32: current volume 0-4 */
	int default_vol;                  /* +36: initial volume from DT */
};

/* --- Gain table: volume (0-4) -> GPIO values for gainsel_1/2/3 --- */

static const u8 gain_table[5][3] = {
	/* { g1, g2, g3 } */
	{ 0, 0, 0 },  /* vol 0: minimum gain */
	{ 0, 0, 1 },  /* vol 1 */
	{ 0, 1, 0 },  /* vol 2 */
	{ 0, 1, 1 },  /* vol 3: default */
	{ 1, 0, 0 },  /* vol 4: maximum gain */
};

static void uc_codec_set_gain(struct uc_codec_priv *priv, int vol)
{
	if (vol < 0 || vol > 4)
		return;

	priv->volume = vol;

	if (priv->gainsel_1_gpio)
		gpiod_set_value_cansleep(priv->gainsel_1_gpio,
					 gain_table[vol][0]);
	if (priv->gainsel_2_gpio)
		gpiod_set_value_cansleep(priv->gainsel_2_gpio,
					 gain_table[vol][1]);
	if (priv->gainsel_3_gpio)
		gpiod_set_value_cansleep(priv->gainsel_3_gpio,
					 gain_table[vol][2]);
}

/* --- ALSA kcontrols (volume) --- */

static int snd_volctl_uc_codec_info(struct snd_kcontrol *kcontrol,
				    struct snd_ctl_elem_info *uinfo)
{
	uinfo->type = SNDRV_CTL_ELEM_TYPE_INTEGER;
	uinfo->count = 1;
	uinfo->value.integer.min = 0;
	uinfo->value.integer.max = 4;
	return 0;
}

static int snd_volctl_uc_codec_get(struct snd_kcontrol *kcontrol,
				   struct snd_ctl_elem_value *ucontrol)
{
	struct snd_soc_component *component =
		snd_soc_kcontrol_component(kcontrol);
	struct uc_codec_priv *priv =
		snd_soc_component_get_drvdata(component);

	ucontrol->value.integer.value[0] = priv->volume;
	return 0;
}

static int snd_volctl_uc_codec_put(struct snd_kcontrol *kcontrol,
				   struct snd_ctl_elem_value *ucontrol)
{
	struct snd_soc_component *component =
		snd_soc_kcontrol_component(kcontrol);
	struct uc_codec_priv *priv =
		snd_soc_component_get_drvdata(component);
	int val = ucontrol->value.integer.value[0];

	if (val < 0 || val > 4)
		return 1; /* out of range, no change */

	uc_codec_set_gain(priv, val);
	return 1; /* value changed */
}

static const struct snd_kcontrol_new uc_codec_snd_controls[] = {
	{
		.iface = SNDRV_CTL_ELEM_IFACE_MIXER,
		.name = "DAC Playback Volume",
		.info = snd_volctl_uc_codec_info,
		.get = snd_volctl_uc_codec_get,
		.put = snd_volctl_uc_codec_put,
	},
};

/* --- DAPM widgets and routes --- */

static const struct snd_soc_dapm_widget uc_codec_dapm_widgets[] = {
	SND_SOC_DAPM_OUTPUT("Speaker"),
	SND_SOC_DAPM_DAC("DAC", "Playback", SND_SOC_NOPM, 0, 0),
	SND_SOC_DAPM_INPUT("Mic"),
	SND_SOC_DAPM_ADC("ADC", "Capture", SND_SOC_NOPM, 0, 0),
};

static const struct snd_soc_dapm_route uc_codec_dapm_routes[] = {
	{ "Speaker", NULL, "DAC" },
	{ "ADC", NULL, "Mic" },
};

/* --- DAI ops --- */

/*
 * From disassembly: sdmode GPIO is toggled in startup/shutdown,
 * NOT in DAPM events. Playback start = amp ON, playback stop = amp OFF.
 * Capture start = amp OFF (prevents speaker->mic feedback).
 */

static int uc_codec_daiops_startup(struct snd_pcm_substream *substream,
				   struct snd_soc_dai *dai)
{
	struct snd_soc_component *component = dai->component;
	struct uc_codec_priv *priv =
		snd_soc_component_get_drvdata(component);

	if (!priv->sdmode_gpio)
		return 0;

	if (substream->stream == SNDRV_PCM_STREAM_PLAYBACK)
		gpiod_set_value_cansleep(priv->sdmode_gpio, 1); /* amp ON */
	else
		gpiod_set_value_cansleep(priv->sdmode_gpio, 0); /* amp OFF */

	return 0;
}

static void uc_codec_daiops_shutdown(struct snd_pcm_substream *substream,
				     struct snd_soc_dai *dai)
{
	struct snd_soc_component *component = dai->component;
	struct uc_codec_priv *priv =
		snd_soc_component_get_drvdata(component);

	/* Only turn off amp on playback shutdown */
	if (substream->stream == SNDRV_PCM_STREAM_PLAYBACK && priv->sdmode_gpio)
		gpiod_set_value_cansleep(priv->sdmode_gpio, 0);
}

static const struct snd_soc_dai_ops uc_codec_dai_ops = {
	.startup = uc_codec_daiops_startup,
	.shutdown = uc_codec_daiops_shutdown,
};

/* --- DAI driver --- */

static struct snd_soc_dai_driver uc_codec_dai_driver = {
	.name = "uc-codec-hifi",
	.playback = {
		.stream_name = "Playback",
		.channels_min = 1,
		.channels_max = 2,
		.rates = SNDRV_PCM_RATE_8000_192000,
		.formats = SNDRV_PCM_FMTBIT_S16_LE |
			   SNDRV_PCM_FMTBIT_S24_LE |
			   SNDRV_PCM_FMTBIT_S32_LE,
	},
	.capture = {
		.stream_name = "Capture",
		.channels_min = 1,
		.channels_max = 2,
		.rates = SNDRV_PCM_RATE_8000_192000,
		.formats = SNDRV_PCM_FMTBIT_S16_LE |
			   SNDRV_PCM_FMTBIT_S24_LE |
			   SNDRV_PCM_FMTBIT_S32_LE,
	},
	.ops = &uc_codec_dai_ops,
};

/* --- Component driver --- */

static const struct snd_soc_component_driver uc_codec_component_driver = {
	.controls = uc_codec_snd_controls,
	.num_controls = ARRAY_SIZE(uc_codec_snd_controls),
	.dapm_widgets = uc_codec_dapm_widgets,
	.num_dapm_widgets = ARRAY_SIZE(uc_codec_dapm_widgets),
	.dapm_routes = uc_codec_dapm_routes,
	.num_dapm_routes = ARRAY_SIZE(uc_codec_dapm_routes),
	.idle_bias_on = 1,
	.use_pmdown_time = 1,
	.endianness = 1,
};

/* --- Platform driver --- */

static int uc_codec_platform_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct uc_codec_priv *priv;
	int ret;

	priv = devm_kzalloc(dev, sizeof(*priv), GFP_KERNEL);
	if (!priv)
		return -ENOMEM;

	platform_set_drvdata(pdev, priv);

	/*
	 * From disassembly (0xbda4d4): probe calls devm_gpiod_get with
	 * GPIOD_OUT_HIGH (3) for all GPIOs. The sdmode starts HIGH
	 * but DAI startup/shutdown toggles it.
	 */
	priv->sdmode_gpio = devm_gpiod_get_optional(dev, "sdmode",
						     GPIOD_OUT_LOW);
	if (IS_ERR(priv->sdmode_gpio))
		return dev_err_probe(dev, PTR_ERR(priv->sdmode_gpio),
				     "failed to get sdmode GPIO\n");

	priv->gainsel_1_gpio = devm_gpiod_get_optional(dev, "gainsel_1",
							GPIOD_OUT_HIGH);
	if (IS_ERR(priv->gainsel_1_gpio))
		return dev_err_probe(dev, PTR_ERR(priv->gainsel_1_gpio),
				     "failed to get gainsel_1 GPIO\n");

	priv->gainsel_2_gpio = devm_gpiod_get_optional(dev, "gainsel_2",
							GPIOD_OUT_HIGH);
	if (IS_ERR(priv->gainsel_2_gpio))
		return dev_err_probe(dev, PTR_ERR(priv->gainsel_2_gpio),
				     "failed to get gainsel_2 GPIO\n");

	priv->gainsel_3_gpio = devm_gpiod_get_optional(dev, "gainsel_3",
							GPIOD_OUT_HIGH);
	if (IS_ERR(priv->gainsel_3_gpio))
		return dev_err_probe(dev, PTR_ERR(priv->gainsel_3_gpio),
				     "failed to get gainsel_3 GPIO\n");

	/* Read optional default-volume from DT; default to 3 if absent */
	ret = of_property_read_u32(dev->of_node, "default-volume",
				   &priv->default_vol);
	if (ret || priv->default_vol > 4)
		priv->default_vol = 3;

	/* Set initial gain */
	uc_codec_set_gain(priv, priv->default_vol);

	dev_info(dev, "uctronics codec: sdmode=%s gain=%d\n",
		 priv->sdmode_gpio ? "yes" : "no", priv->volume);

	return devm_snd_soc_register_component(dev,
					       &uc_codec_component_driver,
					       &uc_codec_dai_driver, 1);
}

static const struct of_device_id uc_codec_device_id[] = {
	{ .compatible = "uctronics,uctronics-codec" },
	{ }
};
MODULE_DEVICE_TABLE(of, uc_codec_device_id);

static struct platform_driver uc_codec_platform_driver = {
	.driver = {
		.name = "snd-soc-uctronics-codec",
		.of_match_table = uc_codec_device_id,
	},
	.probe = uc_codec_platform_probe,
};
module_platform_driver(uc_codec_platform_driver);

MODULE_AUTHOR("jhana-rs project");
MODULE_DESCRIPTION("uctronics uc_codec Codec Driver");
MODULE_LICENSE("GPL v2");
