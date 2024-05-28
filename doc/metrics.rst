.. _Metrics:


Metrics
=======

Prometheus configuration
------------------------

diode-send and diode-receive implements a metric system compatible with prometheus.
To enable this system, is it required to provide url which will be scrapped by prometheus.

.. code-block::

   cargo run --release --bin <myapp> -- --metrics 0.0.0.0:1234 ...

Relevant metrics
----------------

diode-send
""""""""""

* tx_sessions            : total number of TCP connections to diode-send
* tx_tcp_blocks          : total number of blocks received on TCP session
* tx_tcp_bytes           : total number of bytes received on TCP session
* tx_encoding_blocks     : total number of blocks successfully encoded
* tx_encoding_blocks_err : total number of blocks lost due to encoding error
* tx_udp_pkts            : total number of UDP packets successfully sent
* tx_udp_bytes           : total number of bytes successfully sent in UDP packets
* tx_udp_pkts_err        : total number of UDP packets not sent (socket error)
* tx_udp_bytes_err       : total number of bytes not sent (socket error)

diode-receive
"""""""""""""

* rx_sessions            : total number of completed TCP sessions
* rx_decoding_blocks     : total number of blocks successfully decoded
* rx_decoding_blocks_err : total number of blocks lost due to decoding error
* rx_udp_pkts            : total number of UDP packets successfully received 
* rx_udp_bytes           : total number of bytes successfully received from UDP packets
* rx_udp_pkts_err        : total number of UDP packets not received (socket error)
* rx_tcp_blocks          : total number of blocks sent on TCP session
* rx_tcp_blocks_err      : total number of lost blocks, not sent on TCP session (socket error)
* rx_tcp_bytes           : total number of bytes sent on TCP session
* rx_tcp_bytes_err       : total number of lost bytes, not sent on TCP session (socket error)

TODO: document reorder metrics
