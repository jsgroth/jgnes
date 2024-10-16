# jgnes

**Note: This emulator has been largely obsoleted by the NES core in [my multi-system emulator](https://github.com/jsgroth/jgenesis), but this emulator does still support a few features that one does not:**
* A configurable Gaussian blur filter, which is not accurate to any TV but does smooth out the image with some "fancy" blur
* The entirety of the web frontend; jgenesis has a web frontend, but it doesn't support NES and there's no support for input configuration

---

A cross-platform NES emulator. Has a native frontend built using SDL2 as well as a [web frontend](https://jsgroth.dev/jgnes/) that compiles to WASM and runs in the browser.

## Feature Status

Implemented:
* Cycle-based 6502 CPU emulation, PPU emulation (graphics), and APU emulation (audio)
* The most commonly used cartridge boards (aka mappers)
  * NROM
  * The most common NROM variants (UxROM / CNROM / AxROM)
  * MMC1
  * MMC3/MMC6
* Some less commonly used cartridge boards
  * MMC2/MMC4
  * MMC5 including expansion audio
  * Konami VRC2 / VRC4 / VRC6 / VRC7, including VRC6 expansion audio and VRC7 expansion audio
  * Sunsoft 5A / 5B / FME-7, including 5B expansion audio
  * Codemasters unlicensed board
  * Color Dreams unlicensed board
  * Bandai FCG-1 / FCG-2 / LZ93D50
    * The LZ93D50 variant that uses the Datach Joint ROM system is not implemented (iNES mapper 157)
  * Namco 108 / 129 / 163 / 175 / 340, including 163 expansion audio
  * NAMCOT-3425 / NAMCOT-3446 / NAMCOT-3453
  * Jaleco JF-11 / JF-14
  * GxROM
  * BNROM
  * NINA-001
* P1 & P2 input with support for keyboard input and DirectInput gamepad input
* Support for 3 different forced aspect ratios (NTSC, 1:1 pixel aspect ratio, 4:3 screen aspect ratio), plus an option for stretched/none
* Overscan customization
* A GPU-backed renderer based on `wgpu` with an option for integer upscaling + linear interpolation, producing a sharp but clean image even at higher resolutions and non-8:7 aspect ratios
* Save & load state
* Fast forward with a configurable speed multiplier (very CPU-intensive at higher multipliers; my i5-1240P laptop maxes out between 5x and 6x speed)
* Rewind
* Support for both NTSC and PAL releases

Not Implemented:
* A handful of unofficial CPU opcodes that are buggy/unstable and do not do anything useful (specifically $93, $9B, and $9F)
* Cycle-accurate rendering effects of enabling rendering mid-scanline
* DMC DMA cycle stealing and dummy reads, an obscure hardware quirk that affects very very few if any games
* Color palette customization; the NES hardware directly outputs an NTSC video signal rather than RGB pixel grids, so any mapping from NES colors to RGB colors is an approximation at best
* Lots of more obscure cartridge boards
* Support for any controller port peripherals (e.g. the Zapper)

## Crate Structure

* `jgnes-proc-macros`: Custom derive macros used in `jgnes-core`.
* `jgnes-core`: The emulation core. Has few dependencies and requires external code to drive it. Uses callbacks for rendering frames, playing audio, polling for input, and writing save files.
* `jgnes-renderer`: Code for the GPU-backed renderer, as well as some common configuration code that is shared by both renderers.
* `jgnes-native-driver`: Emulator driver that uses SDL2 to handle everything related to video/audio/input, with an option to use either the GPU renderer or an SDL2 software renderer for rendering emulator output into the window.
* `jgnes-cli`: A command-line interface that invokes `jgnes-native-driver`.
* `jgnes-gui`: A graphical user interface that invokes `jgnes-native-driver`.
* `jgnes-web`: An experimental WASM+WebGL2 frontend for `jgnes-core` that runs in the browser. Uses `winit` to create the frame and `jgnes-renderer` to render emulator output.

## Requirements

### Rust

This project requires the latest stable version of the [Rust toolchain](https://doc.rust-lang.org/book/ch01-01-installation.html) to build.
See link for installation instructions.

### SDL2

This project requires [SDL2](https://libsdl.org/) core headers to build.

Linux (Debian-based):
```shell
sudo apt install libsdl2-dev
```

Windows:
* https://github.com/libsdl-org/SDL/releases

### GTK3 (Linux GUI only)

On Linux only, the GUI requires [GTK3](https://www.gtk.org/) headers to build.

Linux (Debian-based):
```shell
sudo apt install libgtk-3-dev
```

## Build & Run

To build the CLI and run for a given NES ROM file:
```shell
cargo run --release --bin jgnes-cli -- -f /path/to/file.nes
```

To view all CLI args:
```shell
cargo run --release --bin jgnes-cli -- -h
```

To build and run the GUI:
```shell
cargo run --release --bin jgnes-gui
```

## Test ROM Results

### CPU Test ROMs

| Test | Result | Failure Reason |
| --- | --- | --- |
| blargg_nes_cpu_test5/cpu.nes | ✅ | |
| blargg_nes_cpu_test5/official.nes | ✅ | |
| branch_timing_tests/1.Branch_Basics.nes | ✅ | |
| branch_timing_tests/2.Backward_Branch.nes | ✅ | |
| branch_timing_tests/3.Forward_Branch.nes | ✅ | |
| cpu_dummy_reads/cpu_dummy_reads.nes | ✅ | |
| cpu_dummy_writes/cpu_dummy_writes_oam.nes | ✅ | |
| cpu_dummy_writes/cpu_dummy_writes_ppumem.nes | ✅ | |
| cpu_exec_space/test_cpu_exec_space_apu.nes | ✅ | |
| cpu_exec_space/test_cpu_exec_space_ppuio.nes | ✅ | |
| cpu_interrupts_v2/cpu_interrupts.nes | ✅ | |
| cpu_timing_test6/cpu_timing_test.nes | ✅ | |
| instr_test-v5/all_instrs.nes | ✅ | |
| instr_test-v5/official_only.nes | ✅ | |
| instr_timing/instr_timing.nes | ✅ | |

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
| dmc_dma_during_read4/dma_2007_read.nes | ❌ | DMC DMA dummy reads not implemented |
| dmc_dma_during_read4/dma_2007_write.nes | ✅ | |
| dmc_dma_during_read4/dma_4016_read.nes | ❌ | DMC DMA dummy reads not implemented |
| dmc_dma_during_read4/double_2007_read.nes | ❌ | DMC DMA dummy reads not implemented |
| dmc_dma_during_read4/read_write_2007.nes | ✅ | |
| sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes | ❌ | DMC DMA cycle stealing not implemented |
| sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes | ❌ | DMC DMA cycle stealing not implemented |

### PPU Test ROMs

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

### Mapper-Specific Test ROMs

| Test | Result | Failure Reason |
| --- | --- | --- |
| mmc3_test_2/1-clocking.nes | ✅ | |
| mmc3_test_2/2-details.nes | ✅ | |
| mmc3_test_2/3-A12_clocking.nes | ✅ | |
| mmc3_test_2/4-scanline_timing.nes | ✅ | |
| mmc3_test_2/5-MMC3.nes | ✅ | |
| mmc3_test_2/6-MMC3_alt.nes | ❌ | #2: IRQ shouldn't be set when reloading to 0 due to counter naturally reaching 0 previously |

### Reset Test ROMs

| Test | Result | Failure Reason |
| --- | --- | --- |
| apu_reset/4015_cleared.nes | ✅ | |
| apu_reset/4017_timing.nes | ✅ | |
| apu_reset/4017_written.nes | ✅ | |
| apu_reset/irq_flag_cleared.nes | ✅ | |
| apu_reset/len_ctrs_enabled.nes | ✅ | |
| apu_reset/works_immediately.nes | ✅ | |
| cpu_reset/ram_after_reset.nes | ✅ | |
| cpu_reset/registers.nes | ✅ | |

## Screenshots

![Screenshot from 2023-05-06 23-12-07](https://user-images.githubusercontent.com/1137683/236657493-07f070d7-4cc8-4db4-ae0b-196e2c67216e.png)

![Screenshot from 2023-05-06 23-14-04](https://user-images.githubusercontent.com/1137683/236657497-2a62be4a-1700-4f7a-8b0c-5c007fdc5a7b.png)

![Screenshot from 2023-05-06 23-16-40](https://user-images.githubusercontent.com/1137683/236657499-0a6d0604-cae1-4d9a-b222-dd9767b44f4d.png)

![Screenshot from 2023-05-06 23-22-50](https://user-images.githubusercontent.com/1137683/236657502-01801f32-f521-4cd2-a68c-61b3649920b7.png)

![Screenshot from 2023-05-06 23-26-54](https://user-images.githubusercontent.com/1137683/236657588-20db8af6-51aa-430b-b927-fc8335113345.png)

