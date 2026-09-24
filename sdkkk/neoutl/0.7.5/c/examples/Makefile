CC ?= gcc
CFLAGS ?= -O2 -Wall -Wextra -fPIC -I../include

TARGETS = libsample_effect.so libsample_object.so libsample_extension.so

all: $(TARGETS)

libsample_effect.so: sample_effect.c
	$(CC) $(CFLAGS) -shared -o $@ $<

libsample_object.so: sample_object.c
	$(CC) $(CFLAGS) -shared -o $@ $<

libsample_extension.so: sample_extension.c
	$(CC) $(CFLAGS) -shared -o $@ $<

clean:
	rm -f $(TARGETS) *.o

.PHONY: all clean
