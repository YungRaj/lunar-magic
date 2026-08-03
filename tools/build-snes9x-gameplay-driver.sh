#!/bin/sh
set -eu

if [ "$#" -ne 1 ] || [ -e "$1" ]; then
  echo "usage: $0 NEW_OUTPUT_PATH" >&2
  exit 2
fi

case "$(uname -s)" in
  Darwin) dynamic_libs="" ;;
  Linux) dynamic_libs="-ldl" ;;
  *) echo "this driver build currently supports macOS and Linux" >&2; exit 2 ;;
esac

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cc -std=c11 -O2 -Wall -Wextra -Werror "$script_dir/snes9x-gameplay-driver.c" \
  -o "$1" $dynamic_libs
