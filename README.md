# Raspberry Pi Pico USB Keyboard in Rust

Like [rmk](https://github.com/haobogu/rmk), this is an implementation of a USB keyboard using the [Embassy framework](https://embassy.dev/).

Unlike rmk, this is not a framework for keyboards; it's a minimal implementation for [one specific board](https://www.tindie.com/products/tsprlng/mini-orthocurvular-keyboard-pcb/).

Having fewer modules makes it easier to read, and the lack of framework limitations makes it possible to define custom logic more precisely.

A lot of the code is lifted straight from Embassy examples, though I've tried to neaten it up a bit.

This is an implementation of a "40%" layout -- 24 keys per hand -- with extra "layers" to bring the symbols/numbers and/or navigation keys onto the positions of the normal letter keys. The hardware looks like this:

![Photo of keyboard](https://www.tspurling.co.uk/computer-keyboards/build-2022.jpg)

Previously I'd done [the same thing in CircuitPython](https://github.com/tsprlng/pi-pico-usb-keyboard), which works just as well and was easier to get going quickly. However, it's nice to use something lower-level for faster startup time, and to have a more straightforward single image to flash.
