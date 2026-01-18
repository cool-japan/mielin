/* Linker script for ARM Cortex-M microcontrollers
 *
 * This linker script defines memory regions and sections for embedded
 * ARM Cortex-M systems with:
 * - Flash memory (non-volatile, for code and read-only data)
 * - RAM memory (volatile, for data and stack)
 * - XIP (Execute-In-Place) support
 *
 * Memory layout example for STM32F0:
 * - Flash: 0x08000000 - 0x08007FFF (32KB)
 * - RAM:   0x20000000 - 0x20001FFF (8KB)
 */

/* Entry point */
ENTRY(Reset_Handler)

/* Memory regions - customize for your target */
MEMORY
{
    /* Flash memory: code and read-only data */
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 32K

    /* RAM memory: data, bss, heap, stack */
    RAM (rwx)   : ORIGIN = 0x20000000, LENGTH = 8K
}

/* Stack size - adjust based on your needs */
_stack_size = 2K;

/* Heap size - adjust based on your needs */
_heap_size = 4K;

/* Sections */
SECTIONS
{
    /* === FLASH SECTIONS (ROM-able) === */

    /* Vector table must be at the start of flash */
    .vector_table :
    {
        . = ALIGN(4);
        _svector_table = .;
        KEEP(*(.vector_table .vector_table.*))
        _evector_table = .;
    } > FLASH

    /* Code section (.text) - executable code in flash */
    .text :
    {
        . = ALIGN(4);
        _stext = .;

        /* Reset handler first */
        *(.text.Reset_Handler)

        /* All other code */
        *(.text .text.*)

        /* Glue code for ARM/Thumb interworking */
        *(.glue_7 .glue_7t)

        /* Compiler-generated code */
        *(.rodata .rodata.*)

        /* C++ constructors/destructors */
        . = ALIGN(4);
        KEEP(*(.init))
        KEEP(*(.fini))

        . = ALIGN(4);
        _etext = .;
    } > FLASH

    /* ARM exception handling sections */
    .ARM.extab :
    {
        *(.ARM.extab* .gnu.linkonce.armextab.*)
    } > FLASH

    .ARM.exidx :
    {
        __exidx_start = .;
        *(.ARM.exidx* .gnu.linkonce.armexidx.*)
        __exidx_end = .;
    } > FLASH

    /* Read-only data (.rodata) - constants in flash */
    .rodata :
    {
        . = ALIGN(4);
        _srodata = .;
        *(.rodata .rodata.*)
        . = ALIGN(4);
        _erodata = .;
    } > FLASH

    /* === RAM SECTIONS (copied from flash at startup) === */

    /* Initialized data section (.data) */
    .data :
    {
        . = ALIGN(4);
        _sdata = .;
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > FLASH

    /* LMA (Load Memory Address) of .data in flash */
    _sidata = LOADADDR(.data);

    /* Uninitialized data section (.bss) */
    .bss :
    {
        . = ALIGN(4);
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    /* Heap section */
    .heap (NOLOAD) :
    {
        . = ALIGN(4);
        _sheap = .;
        . = . + _heap_size;
        . = ALIGN(4);
        _eheap = .;
    } > RAM

    /* Stack section (grows downward) */
    .stack (NOLOAD) :
    {
        . = ALIGN(8);
        _sstack = .;
        . = . + _stack_size;
        . = ALIGN(8);
        _estack = .;
    } > RAM

    /* Symbols for runtime use */
    _stack_start = _estack;
    _heap_start = _sheap;
    _heap_end = _eheap;

    /* Discard debug sections (can be kept for debugging) */
    /DISCARD/ :
    {
        *(.debug*)
        *(.comment)
        *(.note*)
    }
}

/* Provide symbols for startup code */
PROVIDE(_stack_top = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_ram_start = ORIGIN(RAM));
PROVIDE(_ram_end = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_flash_start = ORIGIN(FLASH));
PROVIDE(_flash_end = ORIGIN(FLASH) + LENGTH(FLASH));
