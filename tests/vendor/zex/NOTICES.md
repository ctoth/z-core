# ZEXDOC notices

`zexdoc.com` is the documented-flags Z80 Instruction Set Exerciser written by
Frank D. Cringle.

- Source repository: https://github.com/agn453/ZEXALL
- Pinned source commit: `8f71d418bae69a476a5a0e5c6e122c8801b8d9f4`
- Pinned binary URL: https://raw.githubusercontent.com/agn453/ZEXALL/8f71d418bae69a476a5a0e5c6e122c8801b8d9f4/zexdoc.com
- SHA-256: `34923a7ed82285d3038b2d54bd64899e12173eebb61f9d07b4fc72e78af2ae8f`

The upstream repository states that ZEXDOC and its source were extracted from
YAZE-AG 2.51.3 and are licensed under the GNU General Public License version
2.0. The upstream license text is available at:
https://github.com/agn453/ZEXALL/blob/8f71d418bae69a476a5a0e5c6e122c8801b8d9f4/LICENSE

This repository executes `zexdoc.com` as test data; it does not link the binary
into z-core.

## Z180-compatible derivative

`zexdoc-z180.com` is a size-preserving derivative of that exact pinned stock
binary for execution on a Z180. The unmodified stock binary remains beside it
as provenance.

- Derived source: `zexdoc-z180.mac`, copied from upstream `zexdoc.mac` at
  commit `8f71d418bae69a476a5a0e5c6e122c8801b8d9f4`. Line endings and upstream
  trailing whitespace are normalized; assembly content is edited only in the
  `tests` pointer table.
- Stock SHA-256:
  `34923a7ed82285d3038b2d54bd64899e12173eebb61f9d07b4fc72e78af2ae8f`.
- Derived SHA-256:
  `349f67340953ed359692ccda23bae7dca9ea64fa766427ae0a4f2de2301ea588`.
- Binary transformation: compact the retained descriptor pointers within file
  offsets `0x3A..0xC1`, then zero-fill the unused words so the table remains
  exactly 136 bytes. Bytes before offset `0x3A` and from offset `0xC2` onward
  are byte-identical to the pinned stock binary.
- Retained descriptors: 58. Omitted descriptors: exactly these nine families
  containing Z80 opcodes that UM0050 defines as undefined on Z180:
  `alu8rx`, `incxh`, `incxl`, `incyh`, `incyl`, `ld8ixy`, `ld8rrx`,
  `rotxy`, and `rotz80`. The first seven exercise IXH/IXL/IYH/IYL. The last
  two sweep the shift/rotate groups and therefore include SLL.
- Source-table padding: the nine removed pointer words are replaced after the
  first zero terminator, preserving every descriptor and later address.

The derivative remains GPLv2 test-program data. It is executed by the emulator
and is not linked into z-core.
