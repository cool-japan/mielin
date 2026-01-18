/* Linker script for RISC-V 32-bit embedded systems
 *
 * This linker script defines memory regions and sections for embedded
 * RISC-V systems with:
 * - Flash memory (non-volatile, for code and read-only data)
 * - RAM memory (volatile, for data and stack)
 * - XIP (Execute-In-Place) support
 *
 * Memory layout example for ESP32-C3:
 * - Flash: 0x42000000 - 0x42060000 (384KB)
 * - RAM:   0x3FC80000 - 0x3FCE4000 (400KB)
 */

/* Entry point */
ENTRY(_start)

/* Memory regions - customize for your target */
MEMORY
{
    /* Flash memory: code and read-only data */
    FLASH (rx)  : ORIGIN = 0x42000000, LENGTH = 384K

    /* RAM memory: data, bss, heap, stack */
    RAM (rwx)   : ORIGIN = 0x3FC80000, LENGTH = 400K
}

/* Stack size */
_stack_size = 32K;

/* Heap size */
_heap_size = 256K;

/* Sections */
SECTIONS
{
    /* === FLASH SECTIONS (ROM-able) === */

    /* Code section (.text) */
    .text :
    {
        . = ALIGN(4);
        _stext = .;

        /* Entry point */
        KEEP(*(.text.start))
        KEEP(*(.text._start))

        /* All code */
        *(.text .text.*)

        /* Trap vectors */
        KEEP(*(.trap))

        . = ALIGN(4);
        _etext = .;
    } > FLASH

    /* Read-only data (.rodata) */
    .rodata :
    {
        . = ALIGN(4);
        _srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(4);
        _erodata = .;
    } > FLASH

    /* === RAM SECTIONS === */

    /* Initialized data section (.data) */
    .data :
    {
        . = ALIGN(4);
        _sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > FLASH

    /* LMA of .data in flash */
    _sidata = LOADADDR(.data);

    /* Small data section (RISC-V specific) */
    .sdata :
    {
        . = ALIGN(4);
        _ssdata = .;
        __global_pointer$ = . + 0x800;
        *(.sdata .sdata.*)
        . = ALIGN(4);
        _esdata = .;
    } > RAM AT > FLASH

    /* Uninitialized data section (.bss) */
    .bss :
    {
        . = ALIGN(4);
        _sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    /* Heap section */
    .heap (NOLOAD) :
    {
        . = ALIGN(8);
        _sheap = .;
        . = . + _heap_size;
        . = ALIGN(8);
        _eheap = .;
    } > RAM

    /* Stack section (grows downward) */
    .stack (NOLOAD) :
    {
        . = ALIGN(16);  /* RISC-V requires 16-byte alignment */
        _sstack = .;
        . = . + _stack_size;
        . = ALIGN(16);
        _estack = .;
    } > RAM

    /* Symbols for runtime use */
    _stack_start = _estack;
    _heap_start = _sheap;
    _heap_end = _eheap;

    /* Discard debug sections */
    /DISCARD/ :
    {
        *(.debug*)
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}

/* Provide symbols */
PROVIDE(_stack_top = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_ram_start = ORIGIN(RAM));
PROVIDE(_ram_end = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_flash_start = ORIGIN(FLASH));
PROVIDE(_flash_end = ORIGIN(FLASH) + LENGTH(FLASH));
