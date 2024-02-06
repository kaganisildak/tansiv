# Compilation flags
CFLAGS=-Wall -Wextra
CPPFLAGS=-idirafter /opt/tansiv/include 
LDFLAGS=-ltanqemuxen -ltansiv-timer-xen -lvmi -lxenctrl -L/opt/tansiv/lib/

.PHONY: all libvmi_tansiv clean help

all: libvmi_tansiv

libvmi_tansiv: libvmi_tansiv.o
	$(CC) $(CFLAGS) $(CPPFLAGS) -o $@ $^ $(LDFLAGS)

libvmi_tansiv.o: libvmi_tansiv.c
	$(CC) $(CFLAGS) $(CPPFLAGS) -c $<

clean:
	-rm -f libvmi_tansiv *.o *~

help:
	@echo "Usage:"
	@echo "make all\t\tRun the whole build of libvmi_tansiv"
	@echo "make libvmi_tansiv\tBuild the executable file from libvmi_tansiv.c"
	@echo "make clean\t\tRemove all the files produced by the compilation"
	@echo "make help\t\tDisplay this help message"