.. _Getting started:

Getting started
===============

Installation
------------

Building from scratch
^^^^^^^^^^^^^^^^^^^^^

Prerequisites
"""""""""""""

The following dependencies are needed in order to build lidi from scratch.

- `rust` and `cargo`

The usual way to install the rust toolchain is to firstly install the tool `rustup`.
Once `rustup` is available, you can simply run:

.. code-block::

   $ rustup install stable

Building
""""""""

Building lidi is fairly easy once you have all the dependencies set-up:

.. code-block::

   $ cargo build --release

This step provides you with the two main binaries for lidi: the sender and the receiver part, in addition to other utility binaries, such as file sending/receiving ones.

Setting up a simple case
------------------------

The simplest case we can set up is to have lidi sender and receiver part running on the same machine. Next, we will use `netcat` tool to actually send and receive data over the (software) diode link.

In a first terminal, we start by running the sender part of lidi with default parameters:

.. code-block::

   $ cargo run --release --bin diode-send

Some information logging should will show up, especially indicating that the diode is waiting for TCP connections on port 5000 and that the traffic will go through the diode on UDP port 6000.

Next, we run the receiving part of lidi:

.. code-block::
  
   $ cargo run --release --bin diode-receive -- --to_tcp 127.0.0.1:7000

This time, logging will indicate that traffic will come up on UDP port 6000 and that transferred content will be served on TCP port 7000.

.. note::
   Warning messages about the receiver not receiving the heartbeat message may appear on the receiving part terminal. For example, this is the case if the receiver part is launched several seconds before the sender part is run.
   If it is the case, double check that the sender part is still running and that ip addresses and ports for the UDP traffic are the same on the two parts.

The diode is now waiting for TCP connections to send and receive data.
We run a first netcat instance waiting for connection on port 7000 with the following command:

.. code-block::

   $ nc -lv 127.0.0.1 7000

Finally, we should be able to connect and send raw data through the diode in a fourth terminal:

.. code-block::

   $ nc 127.0.0.1 5000
   Hello Lidi!
   <Ctrl-D>

The message should have been transferred with only forwarding UDP traffic, to finally show up in the first waiting netcat terminal window!

Next steps is to review :ref:`Command line parameters` to adapt them to your use case, and eventually :ref:`Tweaking parameters` to achieve optimal transfer performances.
