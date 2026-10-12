#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <stdint.h>
#include <string.h>

// Cumulative bytes for one BSD disk name. Partition names inherit the
// IOBlockStorageDriver totals of the physical disk they sit on.
typedef struct {
    char bsd_name[32];
    uint64_t bytes_read;
    uint64_t bytes_written;
} DiskIoCounter;

static uint64_t number_value(CFTypeRef value) {
    long long parsed = 0;
    if (value == NULL || CFGetTypeID(value) != CFNumberGetTypeID()) {
        return 0;
    }
    if (!CFNumberGetValue((CFNumberRef)value, kCFNumberLongLongType, &parsed) || parsed < 0) {
        return 0;
    }
    return (uint64_t)parsed;
}

static int already_recorded(const DiskIoCounter *out, int count, const char *name) {
    for (int index = 0; index < count; index++) {
        if (strcmp(out[index].bsd_name, name) == 0) {
            return 1;
        }
    }
    return 0;
}

static int push_name(DiskIoCounter *out, int capacity, int count, const char *name, uint64_t bytes_read, uint64_t bytes_written) {
    if (name == NULL || name[0] == '\0' || count >= capacity || already_recorded(out, count, name)) {
        return count;
    }
    strncpy(out[count].bsd_name, name, sizeof(out[count].bsd_name) - 1);
    out[count].bsd_name[sizeof(out[count].bsd_name) - 1] = '\0';
    out[count].bytes_read = bytes_read;
    out[count].bytes_written = bytes_written;
    return count + 1;
}

static int record_bsd_name(io_registry_entry_t entry, DiskIoCounter *out, int capacity, int count, uint64_t bytes_read, uint64_t bytes_written) {
    char name[32];
    CFTypeRef property = IORegistryEntryCreateCFProperty(entry, CFSTR("BSD Name"), kCFAllocatorDefault, 0);
    if (property == NULL) {
        return count;
    }
    name[0] = '\0';
    if (CFGetTypeID(property) == CFStringGetTypeID()) {
        if (!CFStringGetCString((CFStringRef)property, name, (CFIndex)sizeof(name), kCFStringEncodingUTF8)) {
            name[0] = '\0';
        }
    }
    CFRelease(property);
    return push_name(out, capacity, count, name, bytes_read, bytes_written);
}

static void walk_children(io_registry_entry_t entry, DiskIoCounter *out, int capacity, int *count, uint64_t bytes_read, uint64_t bytes_written, int depth) {
    io_iterator_t children = 0;
    io_object_t child;

    if (depth > 8) {
        return;
    }
    if (IORegistryEntryGetChildIterator(entry, kIOServicePlane, &children) != KERN_SUCCESS) {
        return;
    }
    while ((child = IOIteratorNext(children)) != 0) {
        *count = record_bsd_name(child, out, capacity, *count, bytes_read, bytes_written);
        walk_children(child, out, capacity, count, bytes_read, bytes_written, depth + 1);
        IOObjectRelease(child);
    }
    IOObjectRelease(children);
}

int mac_stats_sample_disk_io(DiskIoCounter *out, int capacity) {
    CFMutableDictionaryRef matching;
    io_iterator_t services = 0;
    io_object_t service;
    int count = 0;

    if (out == NULL || capacity <= 0) {
        return 0;
    }
    matching = IOServiceMatching("IOBlockStorageDriver");
    if (matching == NULL) {
        return 0;
    }
    if (IOServiceGetMatchingServices(kIOMainPortDefault, matching, &services) != KERN_SUCCESS) {
        return 0;
    }

    while ((service = IOIteratorNext(services)) != 0) {
        CFTypeRef stats = IORegistryEntryCreateCFProperty(service, CFSTR("Statistics"), kCFAllocatorDefault, 0);
        uint64_t bytes_read = 0;
        uint64_t bytes_written = 0;
        if (stats != NULL && CFGetTypeID(stats) == CFDictionaryGetTypeID()) {
            bytes_read = number_value(CFDictionaryGetValue((CFDictionaryRef)stats, CFSTR("Bytes (read)")));
            bytes_written = number_value(CFDictionaryGetValue((CFDictionaryRef)stats, CFSTR("Bytes (write)")));
        }
        if (stats != NULL) {
            CFRelease(stats);
        }
        count = record_bsd_name(service, out, capacity, count, bytes_read, bytes_written);
        walk_children(service, out, capacity, &count, bytes_read, bytes_written, 0);
        IOObjectRelease(service);
    }
    IOObjectRelease(services);
    return count;
}