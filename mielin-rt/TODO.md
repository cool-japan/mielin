# mielin-rt TODO

## Pending Tasks

### High Priority
- [ ] Test on STM32 (Cortex-M4/M7)
- [ ] Test on ESP32 (Xtensa/RISC-V)
- [ ] Test on nRF52 (Cortex-M4)
- [ ] Test on RP2040 (Cortex-M0+)
- [ ] Power consumption profiling on real hardware
- [ ] Battery life validation

### Medium Priority
- [x] Support for more MCU families — RISC-V IMAC (rv32imac/rv64imac, CLINT/PLIC, ESP32-C3, GD32VF103, VisionFive2) + ARMv8-M (Cortex-M23/M33/M55, TrustZone SAU, enhanced MPU); 66 tests in riscv/ + armv8m/

### Low Priority
- [ ] Video tutorials for embedded development

## Completed Features (v0.1.0-rc.1)

The following major features have been implemented and are production-ready:

### Embedded Runtime
- ✅ Lightweight runtime for Cortex-M
- ✅ Integration with mielin-kernel
- ✅ Integration with mielin-hal
- ✅ no_std support

### MCU Support
- ✅ STM32 family support (preliminary)
- ✅ ESP32 support (preliminary)
- ✅ nRF52 support (preliminary)
- ✅ RP2040 support (preliminary)

### Real-Time Features
- ✅ GPIO operations
- ✅ Timer support
- ✅ Interrupt handling
- ✅ Power management primitives

### Resource Optimization
- ✅ Memory-efficient design
- ✅ Low power consumption optimization
- ✅ Small binary footprint

### Testing & Documentation
- ✅ Test suite for embedded features
- ✅ Examples for common MCUs
- ✅ Documentation

For detailed feature descriptions and API documentation, see [README.md](README.md) and the generated rustdoc.
