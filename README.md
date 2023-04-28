# jgnes

The early stages of what might some day be a functional NES emulator

Implemented:
* Cycle-based 6502 CPU emulation
  * Unofficial opcodes not implemented
  * Open bus is not implemented; open bus reads always return $FF
* Cycle-based PPU emulation (graphics)
  * Rendering effects of mid-scanline writes may not be cycle-accurate based on non-binary test ROMs (nmi_sync / scanline)
* Cycle-based APU emulation (audio)
  * DMC DMA cycle stealing is not implemented
  * DMC IRQ timing may not be cycle-accurate based on non-binary test ROMs (dpcmletterbox). This might also just be related to DMC DMA cycle stealing not being implemented
* The most commonly used mappers
  * NROM
  * UxROM
  * CNROM
  * AxROM
  * MMC1
  * MMC3
    * MMC3 IRQs are implemented but have some timing issues, some games may have (even more) visual glitches than they do on actual hardware
* P1 input with hardcoded keys

Not Implemented:
* An option to scale the native 8:7 NES output to 4:3, as TVs would have done back in the 1980s
* A smarter way of mapping NES colors to RGB colors; currently using a hardcoded palette that looks kind of ok, color emphasis is not currently implemented
* Overscan customization; some games look really bad without cropping ~8 columns of pixels off each side of the screen
* Lots of mappers, most notably MMC2 (Punch-Out!!), MMC5 (e.g. Castlevania 3), MMC6 (StarTropics 1 & 2), and Konami's VRC mappers
* P2 input and input configuration (or any configuration really)
* Save file persistence for cartridges with both PRG RAM and a battery
* RESET button functionality
* A GUI

## Test ROM Results

### CPU Test ROMs

| Test | Result | Failure Reason |
| --- | --- | --- |
| blargg_nes_cpu_test5/cpu.nes | ❌ | Panic due to unofficial opcodes |
| blargg_nes_cpu_test5/official.nes | ✅ | |
| branch_timing_tests/1.Branch_Basics.nes | ✅ | |
| branch_timing_tests/2.Backward_Branch.nes | ✅ | |
| branch_timing_tests/3.Forward_Branch.nes | ✅ | |
| cpu_dummy_reads/cpu_dummy_reads.nes | ✅ | |
| cpu_dummy_writes/cpu_dummy_writes_oam.nes | ❌ | Panic due to unofficial opcodes |
| cpu_dummy_writes/cpu_dummy_writes_ppumem.nes | ❌ | Panic due to unofficial opcodes |
| cpu_exec_space/test_cpu_exec_space_apu.nes | ❌ | Panic due to CPU open bus not being implemented, leading to invalid opcode execution |
| cpu_exec_space/test_cpu_exec_space_ppuio.nes | ✅ | |
| cpu_interrupts_v2/cpu_interrupts.nes | ✅ | |
| cpu_timing_test6/cpu_timing_test.nes | ✅ | |
| instr_test-v5/all_instrs.nes | ❌ | Panic due to unofficial opcodes |
| instr_test-v5/official_only.nes | ✅ | |
| instr_timing/instr_timing.nes | ❌ | Panic due to unofficial opcodes |

### APU Test ROMs

| Test | Result | Failure Reason |
| --- | --- | --- |
| apu_test/apu_test.nes | ✅ | |
| blargg_apu_2005.07.30/01.len_ctr.nes | ✅ | |
| blargg_apu_2005.07.30/02.len_table.nes | ✅ | |
| blargg_apu_2005.07.30/03.irq_flag.nes | ✅ | |
| blargg_apu_2005.07.30/04.clock_jitter.nes | ✅ | |
| blargg_apu_2005.07.30/05.len_timing_mode0.nes | ✅ | |
| blargg_apu_2005.07.30/06.len_timing_mode1.nes | ✅ | |
| blargg_apu_2005.07.30/07.irq_flag_timing.nes | ✅ | |
| blargg_apu_2005.07.30/08.irq_timing.nes | ✅ | |
| blargg_apu_2005.07.30/09.reset_timing.nes | ✅ | |
| blargg_apu_2005.07.30/10.len_halt_timing.nes | ✅ | |
| blargg_apu_2005.07.30/11.len_reload_timing.nes | ❌ | #5: Reload during length clock when ctr > 0 should be ignored |
| dmc_dma_during_read4/dma_2007_read.nes | ❌ | Unknown |
| dmc_dma_during_read4/dma_2007_write.nes | ✅ | |
| dmc_dma_during_read4/dma_4016_read.nes | ❌ | Unknown |
| dmc_dma_during_read4/double_2007_read.nes | ❌ | Unknown |
| dmc_dma_during_read4/read_write_2007.nes | ✅ | |
| sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes | ❌ | DMC DMA cycle stealing not implemented |
| sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes | ❌ | DMC DMA cycle stealing not implemented |

### PPU Test ROMS

| Test | Result | Failure Reason |
| --- | --- | --- |
| blargg_ppu_tests_2005.09.15b/palette_ram.nes | ✅ | |
| blargg_ppu_tests_2005.09.15b/power_up_palette.nes | ❌ | #2: Palette differs from table |
| blargg_ppu_tests_2005.09.15b/sprite_ram.nes | ✅ | |
| blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes | ✅ | |
| blargg_ppu_tests_2005.09.15b/vram_access.nes | ✅ | |
| oam_read/oam_read.nes | ✅ | |
| ppu_open_bus/ppu_open_bus.nes | ❌ | #3: Decay value should become zero by one second |
| ppu_read_buffer/test_ppu_read_buffer.nes | ✅ | |
| ppu_vbl_nmi/ppu_vbl_nmi.nes | ✅ | |
| sprite_hit_tests_2005.10.05/01.basics.nes | ✅ | |
| sprite_hit_tests_2005.10.05/02.alignment.nes | ✅ | |
| sprite_hit_tests_2005.10.05/03.corners.nes | ✅ | |
| sprite_hit_tests_2005.10.05/04.flip.nes | ✅ | |
| sprite_hit_tests_2005.10.05/05.left_clip.nes | ✅ | |
| sprite_hit_tests_2005.10.05/06.right_edge.nes | ✅ | |
| sprite_hit_tests_2005.10.05/07.screen_bottom.nes | ✅ | |
| sprite_hit_tests_2005.10.05/08.double_height.nes | ✅ | |
| sprite_hit_tests_2005.10.05/09.timing_basics.nes | ✅ | |
| sprite_hit_tests_2005.10.05/10.timing_order.nes | ✅ | |
| sprite_hit_tests_2005.10.05/11.edge_timing.nes | ✅ | |
| sprite_overflow_tests/1.Basics.nes | ✅ | |
| sprite_overflow_tests/2.Details.nes | ✅ | |
| sprite_overflow_tests/3.Timing.nes | ✅ | |
| sprite_overflow_tests/4.Obscure.nes | ✅ | |
| sprite_overflow_tests/5.Emulator.nes | ✅ | |
| vbl_nmi_timing/1.frame_basics.nes | ✅ | |
| vbl_nmi_timing/2.vbl_timing.nes | ✅ | |
| vbl_nmi_timing/3.even_odd_frames.nes | ✅ | |
| vbl_nmi_timing/4.vbl_clear_timing.nes | ✅ | |
| vbl_nmi_timing/5.nmi_suppression.nes | ✅ | |
| vbl_nmi_timing/6.nmi_disable.nes | ✅ | |
| vbl_nmi_timing/7.nmi_timing.nes | ✅ | |

## Screenshots

![Screenshot from 2023-04-27 14-38-19](https://user-images.githubusercontent.com/1137683/234973224-c17b6eda-695a-4a13-91c3-111f30be6d0c.png)
