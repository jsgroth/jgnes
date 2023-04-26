# jgnes

The early stages of what might some day be a functional NES emulator

Implemented:
* Cycle-based 6502 CPU emulation
* Cycle-based PPU emulation (graphics)
* Cycle-based APU emulation (audio)
  * Still a few audible issues; the square wave and delta modulation channels occasionally pop, triangle wave and pseudo-random noise channels seem ok
* A few mappers
  * NROM
  * MMC1
  * UxROM
  * CNROM
  * MMC3
    * MMC3 IRQs are implemented but have some timing issues, some games may have (even more) visual glitches than they do on actual hardware
* P1 input with hardcoded keys

Not Implemented:
* An option to scale the native 8:7 NES output to 4:3, as TVs would have done back in the 1980s
* A smarter way of mapping NES colors to RGB colors; currently using a hardcoded palette that looks kind of ok, color emphasis is not currently implemented
* Overscan customization; some games look really bad without cropping ~8 columns of pixels off each side of the screen
* Lots of mappers, including commonly used mappers such as the later MMCs
* P2 input and input configuration
* A GUI
