#!/usr/bin/env bash
set -euo pipefail

ESP_IMG="esp.img"
ISO_IMG="ramos.iso"

if [[ ! -f "$ESP_IMG" ]]; then
  echo "esp.img not found, running tools/build_esp.sh first" >&2
  "$(dirname "$0")/build_esp.sh"
fi

rm -f "$ISO_IMG"
xorriso -as mkisofs -R -f -e "$ESP_IMG" -no-emul-boot -o "$ISO_IMG" .

echo "Created $ISO_IMG using $ESP_IMG"
