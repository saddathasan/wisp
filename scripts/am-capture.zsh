#!/bin/zsh
# S6: screenshots one window (Activity Monitor's) without activating it, every ~(0.12 + $3) s, into $2 as
# <epoch ms before>_<epoch ms after>.png, until $2/STOP exists. Usage: am-capture.zsh <window id> <dir> [sleep s]
zmodload zsh/datetime
mkdir -p $2
ms() { print ${${EPOCHREALTIME/./}[1,13]} }
while [[ ! -e $2/STOP ]]; do
  a=$(ms); screencapture -x -o -l $1 $2/tmp.png; b=$(ms)
  mv $2/tmp.png $2/${a}_${b}.png
  sleep ${3:-0.8}
done
