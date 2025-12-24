# RAMOS meme UEFI OS

RAMOS is a Rust `no_std` UEFI application that pretends to be a tiny operating system. It boots straight into a pixel-drawn shell with a command set, persistent state saved on the EFI System Partition, and a hidden vault for curious users.

## Features
- UEFI x86_64 target built with Rust 2021 and `no_std`.
- Custom heap allocator with a GOP framebuffer UI (8x8 bitmap font, windowed terminal look).
- Interactive shell with history, line editing, and commands: `help`, `about`, `clear`, `echo`, `set`, `get`, `vars`, `history`, `save`, `load`, `reboot`, `shutdown` (plus a vault-only secret).
- Persistence to `EFI/RAMOS/state.txt` storing variables, command history, and hints.
- Ready-to-use ESP image builder and QEMU run command.

## Building
Requirements: `rustup`, `llvm`, `mtools`, `dosfstools`, and optionally `xorriso` for ISO output.

```bash
rustup target add x86_64-unknown-uefi
./tools/build_esp.sh
```

This produces `esp.img` containing `EFI/BOOT/BOOTX64.EFI` and a default `EFI/RAMOS/state.txt` with initial state and breadcrumbs.

To also build a UEFI ISO:
```bash
./tools/build_iso.sh
```

## Running in QEMU
Use OVMF firmware (OVMF_CODE.fd and OVMF_VARS.fd) and the generated `esp.img`:
```bash
qemu-system-x86_64 \
  -machine q35 \
  -m 512 \
  -cpu qemu64 \
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
  -drive if=pflash,format=raw,file=OVMF_VARS.fd \
  -drive format=raw,file=esp.img \
  -net none
```

## Persistence format
`state.txt` is plain text:
```
kv:<key>=<value>
h:<command from history>
hint:1
```
It is loaded at boot and rewritten by `save`. Variables starting with `_` are hidden from `vars` output; one of them holds the vault payload in base64 for those who dig.

## Notes
- RAMOS keeps boot services alive (no ExitBootServices) and relies on UEFI runtime reset for reboot/shutdown.
- The framebuffer UI respects RGB/BGR layouts via GOP.
- Default user/host are `ramos`/`ramos`; customize via `set user <name>` and `set host <name>` then `save`.
