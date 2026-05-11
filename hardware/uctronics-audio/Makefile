# Out-of-tree Makefile for MAX98357A and DMIC audio codec drivers
# For Armbian kernel 6.1.115-vendor-rk35xx on Rock 5A (RK3588S)
#
# These modules are needed for the Uctronics AI in a Box onboard
# speaker (MAX98357A I2S Class D amp) and mic (digital MEMS).
# Armbian does not include these codecs in its kernel config.

KERNEL_BUILD_DIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

obj-m += snd-soc-max98357a.o
obj-m += snd-soc-dmic.o

snd-soc-max98357a-objs := max98357a.o
snd-soc-dmic-objs := dmic.o

all:
	$(MAKE) -C $(KERNEL_BUILD_DIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KERNEL_BUILD_DIR) M=$(PWD) clean

.PHONY: all clean
