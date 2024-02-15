# Mosquitto-bzzz

## This Repo

This repository contains the source code of a Rust firmware for an ESP32-C6-DevKitC-1.  This firmware reads noise
measurements from a noise sensor data.

## Edit and run

This code uses [Rust and other tools](https://esp-rs.github.io/book/installation/riscv.html). I have used Emacs (with eglot)
to edit, but you should be able to survive with Visual Studio Code.

Copy `cfg.toml.COPY_EDIT` to `cfg.toml` and edit the new file with your data.  You can then build and run the project using the
following commands:

```console
cargo b # just build
cargo r # build, flash and run
```

## License

This project is licensed under the terms of the [Apache license 2.0](./LICENSE.txt).

## Author

Jorge D. Ortiz-Fuentes, 2024

## Resources

- [▶️ MongoDB's YouTube channel](https://www.youtube.com/c/MongoDBofficial)
- [🙋🏻‍♂️ Jorge's Mastodon](https://fosstodon.org/@jdortiz)
- [🧑🏻‍💻 Jorge's LinkedIn](https://www.linkedin.com/in/jorgeortiz/)
- [🙋🏻‍♂️ Jorge's Twitter](https://twitter.com/jdortiz)
