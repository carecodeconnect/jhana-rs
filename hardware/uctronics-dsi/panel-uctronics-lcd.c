// SPDX-License-Identifier: GPL-2.0+
/*
 * Minimal DSI panel driver for the Uctronics AI in a Box display (720x1280).
 *
 * Timings extracted from the working Radxa Ubuntu 22.04 image via debugfs.
 * This is a clean-room implementation — no proprietary code.
 *
 * Compatible: "uctronics,uctronics-lcd"
 */

#include <linux/delay.h>
#include <linux/gpio/consumer.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/regulator/consumer.h>

#include <video/mipi_display.h>

#include <drm/drm_mipi_dsi.h>
#include <drm/drm_modes.h>
#include <drm/drm_panel.h>

struct uctronics_panel {
	struct drm_panel panel;
	struct mipi_dsi_device *dsi;
	struct gpio_desc *reset_gpio;
	struct regulator *vdd;
	struct regulator *vccio;
	bool prepared;
};

static inline struct uctronics_panel *to_uctronics(struct drm_panel *panel)
{
	return container_of(panel, struct uctronics_panel, panel);
}

/* 720x1280@60Hz, pixel clock 66 MHz
 * H: hactive=720, hfp=40, hsync=20, hbp=55  (htotal=835)
 * V: vactive=1280, vfp=15, vsync=8, vbp=15   (vtotal=1318)
 */
static const struct drm_display_mode uctronics_mode = {
	.clock = 66000,
	.hdisplay = 720,
	.hsync_start = 720 + 40,
	.hsync_end = 720 + 40 + 20,
	.htotal = 720 + 40 + 20 + 55,
	.vdisplay = 1280,
	.vsync_start = 1280 + 15,
	.vsync_end = 1280 + 15 + 8,
	.vtotal = 1280 + 15 + 8 + 15,
	.width_mm = 62,
	.height_mm = 110,
	.type = DRM_MODE_TYPE_DRIVER | DRM_MODE_TYPE_PREFERRED,
};

static int uctronics_get_modes(struct drm_panel *panel,
			       struct drm_connector *connector)
{
	struct drm_display_mode *mode;

	mode = drm_mode_duplicate(connector->dev, &uctronics_mode);
	if (!mode)
		return -ENOMEM;

	drm_mode_set_name(mode);
	drm_mode_probed_add(connector, mode);
	connector->display_info.width_mm = mode->width_mm;
	connector->display_info.height_mm = mode->height_mm;
	connector->display_info.bus_flags = 0;

	return 1;
}

static int uctronics_prepare(struct drm_panel *panel)
{
	struct uctronics_panel *ctx = to_uctronics(panel);
	int ret;

	if (ctx->prepared)
		return 0;

	if (ctx->vdd) {
		ret = regulator_enable(ctx->vdd);
		if (ret < 0)
			return ret;
	}

	if (ctx->vccio) {
		ret = regulator_enable(ctx->vccio);
		if (ret < 0)
			goto err_vdd;
	}

	msleep(20);

	if (ctx->reset_gpio) {
		gpiod_set_value_cansleep(ctx->reset_gpio, 1);
		msleep(10);
		gpiod_set_value_cansleep(ctx->reset_gpio, 0);
		msleep(25);
	}

	ctx->prepared = true;
	return 0;

err_vdd:
	if (ctx->vdd)
		regulator_disable(ctx->vdd);
	return ret;
}

static int uctronics_enable(struct drm_panel *panel)
{
	struct uctronics_panel *ctx = to_uctronics(panel);

	/* Exit sleep mode and turn on display */
	mipi_dsi_dcs_exit_sleep_mode(ctx->dsi);
	msleep(120);
	mipi_dsi_dcs_set_display_on(ctx->dsi);
	msleep(20);

	return 0;
}

static int uctronics_disable(struct drm_panel *panel)
{
	struct uctronics_panel *ctx = to_uctronics(panel);

	mipi_dsi_dcs_set_display_off(ctx->dsi);
	msleep(20);
	mipi_dsi_dcs_enter_sleep_mode(ctx->dsi);
	msleep(120);

	return 0;
}

static int uctronics_unprepare(struct drm_panel *panel)
{
	struct uctronics_panel *ctx = to_uctronics(panel);

	if (!ctx->prepared)
		return 0;

	if (ctx->reset_gpio)
		gpiod_set_value_cansleep(ctx->reset_gpio, 1);

	if (ctx->vccio)
		regulator_disable(ctx->vccio);
	if (ctx->vdd)
		regulator_disable(ctx->vdd);

	ctx->prepared = false;
	return 0;
}

static const struct drm_panel_funcs uctronics_funcs = {
	.prepare = uctronics_prepare,
	.enable = uctronics_enable,
	.disable = uctronics_disable,
	.unprepare = uctronics_unprepare,
	.get_modes = uctronics_get_modes,
};

static int uctronics_probe(struct mipi_dsi_device *dsi)
{
	struct device *dev = &dsi->dev;
	struct uctronics_panel *ctx;
	int ret;

	ctx = devm_kzalloc(dev, sizeof(*ctx), GFP_KERNEL);
	if (!ctx)
		return -ENOMEM;

	ctx->dsi = dsi;
	mipi_dsi_set_drvdata(dsi, ctx);

	dsi->lanes = 4;
	dsi->format = MIPI_DSI_FMT_RGB888;
	dsi->mode_flags = MIPI_DSI_MODE_VIDEO | MIPI_DSI_MODE_VIDEO_BURST |
			  MIPI_DSI_MODE_LPM | MIPI_DSI_MODE_NO_EOT_PACKET;

	ctx->reset_gpio = devm_gpiod_get_optional(dev, "reset", GPIOD_OUT_HIGH);
	if (IS_ERR(ctx->reset_gpio))
		return dev_err_probe(dev, PTR_ERR(ctx->reset_gpio),
				     "failed to get reset gpio\n");

	ctx->vdd = devm_regulator_get_optional(dev, "vdd");
	if (IS_ERR(ctx->vdd)) {
		if (PTR_ERR(ctx->vdd) != -ENODEV)
			return PTR_ERR(ctx->vdd);
		ctx->vdd = NULL;
	}

	ctx->vccio = devm_regulator_get_optional(dev, "vccio");
	if (IS_ERR(ctx->vccio)) {
		if (PTR_ERR(ctx->vccio) != -ENODEV)
			return PTR_ERR(ctx->vccio);
		ctx->vccio = NULL;
	}

	drm_panel_init(&ctx->panel, dev, &uctronics_funcs,
		       DRM_MODE_CONNECTOR_DSI);

	ret = drm_panel_of_backlight(&ctx->panel);
	if (ret)
		return ret;

	drm_panel_add(&ctx->panel);

	ret = mipi_dsi_attach(dsi);
	if (ret < 0) {
		drm_panel_remove(&ctx->panel);
		return ret;
	}

	dev_info(dev, "uctronics 720x1280 DSI panel attached\n");
	return 0;
}

static void uctronics_remove(struct mipi_dsi_device *dsi)
{
	struct uctronics_panel *ctx = mipi_dsi_get_drvdata(dsi);

	mipi_dsi_detach(dsi);
	drm_panel_remove(&ctx->panel);
}

static const struct of_device_id uctronics_of_match[] = {
	{ .compatible = "uctronics,uctronics-lcd" },
	{ /* sentinel */ }
};
MODULE_DEVICE_TABLE(of, uctronics_of_match);

static struct mipi_dsi_driver uctronics_driver = {
	.probe = uctronics_probe,
	.remove = uctronics_remove,
	.driver = {
		.name = "panel-uctronics-lcd",
		.of_match_table = uctronics_of_match,
	},
};
module_mipi_dsi_driver(uctronics_driver);

MODULE_AUTHOR("jhana-rs project");
MODULE_DESCRIPTION("Uctronics AI in a Box 720x1280 DSI panel driver");
MODULE_LICENSE("GPL v2");
