# The hello world example

It doesn't do much. But you can see the Python side setting up logging and both
Python and Rust doing some logging that gets output (and some that doesn't,
because it is filtered by the level).

## Running

While there are probably other ways too, using the
[maturin](https://crates.io/crates/maturin) tool is probably one of the more
convenient ways.

```sh
# First, create a virtualenv
mkdir venv
virtualenv venv
. ./venv/bin/activate
# Compile and "install" the Rust module into the virtualenv
maturin develop
# Now we can run the example
./hw.py
```
