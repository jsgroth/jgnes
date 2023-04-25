# jgnes

The very early stages of what might some day be a functional NES emulator

Implemented:
* Cycle-based 6502 CPU emulation
* Very basic PPU (graphics) emulation; scanline-based instead of pixel-based, inaccurate and buggy
* APU emulation (audio)
* Very few mappers
  * NROM (mapper 0)
  * MMC1 (mapper 1)
  * UxROM (mapper 2)
* P1 input with hardcoded keys

Not Implemented:
* Accurate PPU emulation (pixel-based, correctly handle interactions involving memory-mapped PPU registers)
* An option to scale the native 8:7 NES output to 4:3, as TVs would have done back in the 1980s
* A smarter way of mapping NES colors to RGB colors; currently using a hardcoded palette that looks kind of ok, color emphasis is not currently implemented
* Lots of mappers, including commonly used mappers such as the MMCs
* P2 input and input configuration
* A GUI
