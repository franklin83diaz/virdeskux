## use:
https://crates.io/crates/x11rb
Feature: 
 - damage: capture just the rectangle changed (see note code *2)
 - xfixes: mouse icon status, clipboard status

#### for compress image use qoi
https://crates.io/crates/qoi
or check for best:
https://crates.io/crates/rapid-qoi

## code note:
1) SIMD for check diff with the previous capture
2) SIMD for sum rest image, for convert pixel no change is [0,0,0,0] for fast compress pixel no changed inside the rectangle.
(in the client no need SIMD fir sum the images)
3) Http/2 (good) or Http/3 (may problem with udp) or webSocket (easy but overhead and need chunk below 1Mb or less)


## Command note:
```
xwd -root -out captura.xwd
xdpyinfo | grep dimensions

glxinfo | grep "OpenGL renderer"
```
```
hexdump captura.xwd -n 100
printf '\xFF' | dd of=captura.xwd bs=1 seek=100 count=1 conv=notrunc
```

```
export DISPLAY=:1

sudo Xorg :1 -config /etc/X11/xorg.conf -noreset -logfile /var/log/Xorg.nvidia.log -nolisten tcp &

dbus-launch --exit-with-session mate-session > /dev/null 2>&1 &

x11vnc -display :1 -nopw -listen 0.0.0.0 -noxdamage &
```
