# lidi

[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Rust/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)
[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Clippy/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)

## What is lidi?

Lidi (leedee) allows you to copy TCP or Unix streams or files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

## Where to find some documentation?

The *user* documentation is available at <https://anssi-fr.github.io/lidi/>, or can be built and opened with:

```
$ cd doc
$ make html
$ xdg-open _build/html/index.html
```

The *developper* documentation can be built and opened by running:

```
$ cargo doc --document-private-items --no-deps --lib --open
```

# running tests

Functional testing using behave

```
$ apt install python3-behave
$ behave --tags=~fail
```

## failures

Failing scenarios:
  features/interrupt.feature:3  Send 3x100KB file with network interrupt, 2 first files lost, last one transmitted
  features/interrupt.feature:12  Send 3x1MB file with network interrupt, 2 first files lost, last one transmitted
  features/interrupt.feature:21  Send 3x100MB file with network interrupt, 2 first files lost, last one transmitted
  features/simple.feature:31  Send a 1M file without drop
  features/simple.feature:38  Send multiple 1M files without drop
  features/simple.feature:49  Send a 1G file without drop

Explanation:
* interrupt tests are failing because of a bug in lidi (see https://github.com/ANSSI-FR/lidi/issues/3 )
* big files in 'simple' test suite are failing because there is no rate limit so there are too many packets dropped
