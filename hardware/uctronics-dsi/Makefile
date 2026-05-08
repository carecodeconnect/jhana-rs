obj-m += panel-ilitek-ili9881c.o

KDIR ?= /lib/modules/$(shell uname -r)/build

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

install: all
	sudo cp panel-ilitek-ili9881c.ko /lib/modules/$(shell uname -r)/kernel/drivers/gpu/drm/panel/
	sudo depmod -a
	echo "panel-ilitek-ili9881c" | sudo tee /etc/modules-load.d/uctronics-panel.conf

.PHONY: all clean install
