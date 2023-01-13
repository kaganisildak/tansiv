obj-m += tansiv-timer.o

PWD := $(CURDIR)

all:
	make CC="gcc" ARCH=x86_64 -C /lib/modules/$(shell uname -r)/build M=$(PWD) modules

clean:
	make -C /lib/modules/$(shell uname -r)/build M=$(PWD) clean
