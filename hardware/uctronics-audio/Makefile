# Out-of-tree Makefile for the Uctronics AI in a Box audio codec.
#
# Builds snd-soc-uctronics-codec.ko for Armbian kernel 6.1.115-vendor-rk35xx
# on the Rock 5A (RK3588S). Pair with uctronics-audio-overlay.dts and the
# device-tree fix for rockchip,trcm-sync-tx-only documented there.
#
# Build on the Rock (aarch64) — not on the dev machine.

KERNEL_BUILD_DIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

obj-m += snd-soc-uctronics-codec.o

all:
	$(MAKE) -C $(KERNEL_BUILD_DIR) M=$(PWD) modules

dtbo: uctronics-audio.dtbo

uctronics-audio.dtbo: uctronics-audio-overlay.dts
	dtc -@ -I dts -O dtb -o $@ $<

clean:
	$(MAKE) -C $(KERNEL_BUILD_DIR) M=$(PWD) clean
	rm -f uctronics-audio.dtbo

.PHONY: all clean dtbo
