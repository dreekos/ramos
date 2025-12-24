#!/usr/bin/env bash
set -euo pipefail

TARGET_DIR="target/x86_64-unknown-uefi/release"
IMG="esp.img"

cargo build --release

EFI_BIN="$TARGET_DIR/ramos.efi"
if [[ ! -f "$EFI_BIN" ]]; then
  EFI_BIN="$TARGET_DIR/ramos"
fi
if [[ ! -f "$EFI_BIN" ]]; then
  echo "Could not find built EFI binary" >&2
  exit 1
fi

rm -f "$IMG"
truncate -s 64M "$IMG"
mkfs.fat -F32 "$IMG"

mmd -i "$IMG" ::/EFI ::/EFI/BOOT ::/EFI/RAMOS
mcopy -i "$IMG" "$EFI_BIN" ::/EFI/BOOT/BOOTX64.EFI

tmp_state=$(mktemp)
cat > "$tmp_state" <<STATE
kv:user=ramos
kv:host=ramos
kv:_vault=UkFNT1N7RjB1bmRfM3ZlbjNfenJfc3Qwbmx5X2luX3RoZV9mdXR1cmV9
h:echo the vault lives under hidden keys
h:echo base64 unlocks forgotten things
hint:1
STATE
mcopy -i "$IMG" "$tmp_state" ::/EFI/RAMOS/state.txt
rm -f "$tmp_state"

echo "Created $IMG with BOOTX64.EFI and state.txt"
