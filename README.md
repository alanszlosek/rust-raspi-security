# Note

This doesn't work yet. I'm currently porting C code from RaspiStill.c and once I can capture a still I'll start morphing towards security camera functionality.

# Welcome

I have a Python-based RasPi security camera project, but I have a hunch that a Rust version would perform better since I'd be working closer to MMAL itself. Plus, I'd like to continue my effort to learn Rust. So here we are.

# Project Goals

* Leverage the lower-level nature of Rust to create a more performant motion detection security camera application
* Try to exceed my Python version: 1080p h264 videos when motion is detected

# Personal Goals

* Learn Rust by engaging in a larger, real-world project
* Have fun

# Features

Nothing yet, I'm still porting mmal-sys and raspivid C code to this project. Haven't even captured a still image yet, but hopefully soon.

# Benefits

Hoping for these:

* Make your own security camera's with Raspberry Pis (models 4, 3 and Zero, hopefully)
* Know for a fact that your videos and images aren't making it to the cloud unless you want them to
* Extreme hackability for the DIYers
