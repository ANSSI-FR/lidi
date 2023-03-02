.. _Command line parameters:

Command line parameters
=======================

When running `diode-send` and `diode-receive` with cargo, command line parameters must appear after after double-hyphen separator. For example, to display all available options for the sender part:

.. code-block::

   $ cargo run --release --bin diode-send -- --help

Overview
--------

Here is a diagram of the components involved in an example usage of lidi, annotated with command line parameters:

.. image:: schema.svg

.. note::
   Parameters that are displayed in the gray box must be the same of both sides (sender and receiver) of lidi.

Following, we provide some details about each command line options.

Adresses and ports
------------------

As shown in the :ref:`Getting started` chapter, default values work well for testing the diode on a single machine. But for real application, ip addresses and ports must be configured properly. There are three points in the diode chain where those settings should be provided.

TCP data source
"""""""""""""""

The diode-send side gets data from TCP connections. It is necessary to specify ip address and port in which TCP connections will be accepted with the following parameter:

.. code-block::

   --from_tcp <ip:port>

Default value is 127.0.0.1:5000.

TCP data destination
""""""""""""""""""""

On the diode-receive side, data will be sent to TCP connected clients. To specify listening ip and TCP port:

.. code-block::

   --to_tcp <ip:port>

Default value is 127.0.0.1:7000.

UDP transfer
""""""""""""

UDP transfer is the core of the diode. Settings ip addresses and port is necessary. On the sender side:

.. code-block::

   --to_udp <ip:port>

describe where to send data and is defaulted to 127.0.0.1:6000, and socket is bound to address and port according to:
  
.. code-block::

   --to_bind <ip:port>

which is defaulted to 0.0.0.0:0. This default value should work in many cases.

On the receiver side, the option:

.. code-block::

   --from_udp <ip:port>

defines ip and port to listen for incoming UDP packets, and should be set to the same value as `--to-udp`.

Block and packet sizes
----------------------

Receiver:

.. code-block::

   --encoding_block_size <nb_bytes>
     Size of RaptorQ block [default: 60000]
  
   --repair_block_size <ratior>
     Size of repair data in bytes [default: 6000]

   --from_udp_mtu <nb_bytes>
     MTU of the incoming UDP link [default: 1500]
  
Sender:

.. code-block::

   --encoding_block_size <nb_bytes>
     Size of RaptorQ block in bytes [default: 60000]
  
   --repair_block_size <ratior>
     Size of repair data in bytes [default: 6000]

   --to_udp_mtu <nb_bytes>
     MTU in bytes of output UDP link [default: 1500]

Multiplexing
------------

Receiver:

.. code-block::

   --nb_multiplex <nb>
     Number of multiplexed transfers [default: 2]
  
Sender:

.. code-block::

   --nb_multiplex <nb>
     Number of multiplexed transfers [default: 2]

   --nb_clients <nb>
     Number of simultaneous transfers [default: 2]
  

Multithreading
--------------

To ensure data integrity through the UDP link, Lidi uses RaptorQ fountain codes. This means that logical block of data need to be encoded (sender side) and then decoded (receiver side). Several threads can be spawned to parallelized such computations, with the following options:

.. code-block::

   --nb_encoding_threads <nb>
     (sender side, default: 2)

   --nb_decoding_threads <nb>
     (receiver side, default: 1).

Timeouts
--------

Since lidi uses UDP protocol to transfer data, blocks and datagrams can be reordered.
Fountain codes are used to ensure data integrity despite possible transfer reordering and losses. Also, it can be harder for the receiving part to know that a particular transfer is done, since an EOF-like marker can be received before the end of the data, or simply lost.
Thus, configurable timeouts are used in lidi to decide when to reset fountain code status:

.. code-block::

   --flush_timeout <nb_milliseconds>
     (receiver side, default: 500)

and when to abort an incomplete incoming transfer:
  
.. code-block::

   --abort_timeout <nb_seconds>
     (receiver side, default: 10)

Heartbeat
---------

Since the purpose of the diode is to only allow one-way data traffic, the sender cannot be aware if a receiver is set up or not. But heartbeat messages are regularly sent through the diode so that the receiver can be aware of a sender disconnection. Heartbeat times can be set with the following option on both sides:

.. code-block::

   --heartbeat <nb_secs>

The default values are 5 seconds for the sender (i.e. a heartbeat message is sent every 5 seconds) and 10 seconds for the receiver (i.e. warnings are displayed whenever during 10 seconds no heartbeat message was received). Due to latency, timeouts and network load, the receiver value must always be greater than the sender value.
