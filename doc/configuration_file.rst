.. _configuration_file:

Configuration file
===========================

When running `diode-send` and `diode-receive` with cargo, a configuration file is mandatory. By default, Lidi loads the file located at `/etc/lidi/config.toml`.

It is possible to change the default configuration file used by Lidi, as explained in :ref:`Command line parameters`

.. code-block::

   $ cargo run --release --bin diode-send -- --config ./lidi.toml

.. _configuration_file_sample:

In the configuration file, there are parameters which must be the same on both side of the diode. They are in the first part of the configuration file. After this common paragraph, there is one part for specific options for the sender application and another for receiver application.

In the following chapters, we will detail all the configuration options, grouped by topics.


Configuration file sample
-------------------------

Here is a sample of the configuration file :

.. code-block::

   # Size of RaptorQ block, in bytes
   encoding_block_size = 60000
   
   # Size of repair data, in bytes
   repair_block_size = 6000
   
   # IP address on diode-receive side used to transfert UDP packets between diode-send and diode-receive
   udp_addr = "127.0.0.1"
   
   # List of ports used to transfert packets between diode-send and diode-receive. There must be one different port per thread.
   udp_port = [ 5000 ]
   
   # MTU of the to use one the UDP link
   udp_mtu = 1500
   
   # heartbeat period in ms
   heartbeat = 1000
   
   # Path to log configuration file
   # log_config = "./lidi_log4rs.yml"
   
   # specific options for diode-send
   [sender]

   # TCP server socket to accept data
   bind_tcp = "127.0.0.1:5001"
   
   # UDP source address to use for client socket in format A.B.C.D:port. It is possible to use port 0 for automatic assignement.
   bind_udp = "127.0.0.1:0"
   
   # ratelimit Lidi output (UDP packets throughput). In Mbit/s.
   max_bandwidth = 100
   
   # prometheus port
   # metrics = "0.0.0.0:9001"
   
   # specific options for diode-receive
   [receiver]
   
   # IP address and port of the TCP server
   to_tcp = "127.0.0.1:5002"
   
   # Timeout before forcing incomplete block recovery (in ms). Default is one time heartbeat interval.
   # block_expiration_timeout = 500
   
   # Time to wait before changing session (in ms). Default is 5 times heartbeat interval.
   # session_expiration_timeout = 5000
   
   # prometheus port
   # metrics = "0.0.0.0:9002"
   
   # core_affinity = [ 1 ]

   # Size of the queue between UDP receiver and block reorder/decoder. Default is 10k packets.
   # udp_packets_queue_size = 10000
   
   # Size of the queue between block reorder/decoder and TCP sender. Default is 1k blocks.
   # tcp_blocks_queue_size = 1000

Options are detailed in the following chapters:

* Mandatory network options
   * `udp_addr`, `udp_port`, `bind_tcp` and `to_tcp` are explained in :ref:`network`
   * `max_bandwidth` is described in :ref:`ratelimit`
* Performance optimization options
   * `encoding_block_size` and `repair_block_size` are explained in :ref:`raptorq` 
   * `udp_mtu` is explained in :ref:`mtu`
   * `core_affinity` is explained in :ref:`affinity`
* Monitoring options
   * `log_config` is explained in :ref:`Logging`. See also :ref:`Command line parameters` change log level on console.
   * `metrics` is detailed in :ref:`Metrics`
* Timers 
   * `heartbeat`, `block_expiration_timeout` and `session_expiration_timeout` are explained in :ref:`timers`

Do not forget there are kernel parameters to set in order to prevent packet drops in kernel. This is explained in :ref:`Tweaking parameters`


