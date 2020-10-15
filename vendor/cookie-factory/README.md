# cookie-factory

[![LICENSE](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Build Status](https://travis-ci.org/rust-bakery/cookie-factory.svg?branch=master)](https://travis-ci.org/rust-bakery/cookie-factory)
[![Crates.io Version](https://img.shields.io/crates/v/cookie-factory.svg)](https://crates.io/crates/cookie-factory)

serialization library built with a combinator design similar to the [nom parser combinators library](https://github.com/geal/nom).

Serializers are built up from single purpose serializers, like `slice`
to write a raw byte slice, or `be_u16` to write a `u16` integer in big
endian form.

Those small serializers can then be assembled by using combinators.
As an example, `all(["abcd", "efgh", "ijkl"].iter().map(string))(output)`
will write `"abcdefghijkl"` to `output`.

Reference documentation is available [here](https://docs.rs/cookie-factory/).
