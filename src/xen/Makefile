# Compilation flags
CFLAGS=-Wall -Wextra
CPPFLAGS=-idirafter /opt/tansiv/include 
LDFLAGS=-lm -ltanxen -ltansiv-timer-xen -lvmi -lxenctrl -L/opt/tansiv/lib/

.PHONY: all xen_tansiv_bridge clean help

all: xen_tansiv_bridge

xen_tansiv_bridge: xen_tansiv_bridge.o
	$(CC) $(CFLAGS) $(CPPFLAGS) -o $@ $^ $(LDFLAGS)

xen_tansiv_bridge.o: xen_tansiv_bridge.c
	$(CC) $(CFLAGS) $(CPPFLAGS) -c $<

clean:
	-rm -f xen_tansiv_bridge *.o *~

help:
	@echo "Usage:"
	@echo "make all\t\tRun the whole build of xen_tansiv_bridge"
	@echo "make xen_tansiv_bridge\tBuild the executable file from xen_tansiv_bridge.c"
	@echo "make clean\t\tRemove all the files produced by the compilation"
	@echo "make help\t\tDisplay this help message"
