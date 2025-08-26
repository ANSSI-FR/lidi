.. _Metrics:

Metrics
=======

Prometheus endpoint configuration
---------------------------------

`diode-send` and `diode-receive` implements a metric system compatible with `Prometheus <https://prometheus.io/>`_.
To enable this system, is it required to provide the url which will be scrapped by prometheus.

There are two urls to set, one for the sender and one for the receiver. They both are under the "metric" option.

.. code-block::

   [sender]
   metrics = "127.0.0.1:9001"

   [receiver]
   metrics = "127.0.0.1:9001"

.. note::

   When running on the same host for tests, it is necessary to put different ports or it will fail with the following error: `Cannot init metrics: cannot start http listener: failed to create HTTP listener: Address already in use`


Relevant metrics
----------------

diode-send
""""""""""

* tx_sessions            : total number of TCP connections accepted by diode-send
* tx_tcp_blocks          : total number of blocks received on TCP sessions
* tx_tcp_bytes           : total number of bytes received on TCP sessions
* tx_encoding_blocks     : total number of blocks successfully encoded
* tx_encoding_blocks_err : total number of blocks lost due to encoding error
* tx_udp_pkts            : total number of UDP packets successfully sent to diode-receive
* tx_udp_bytes           : total number of bytes successfully sent on UDP packets to diode-receive. This only is the udp payload without lidi header, this does not contain network transport headers of packets (Eth/IP/UDP). Since it contains repair packets and one raptorq header per block, the value is bigger than tx_tcp_bytes.
* tx_udp_pkts_err        : total number of UDP packets not sent (socket error)
* tx_udp_bytes_err       : total number of bytes not sent (socket error)

diode-receive
"""""""""""""

All stats of diode-receive starts with `rx`.

* rx_sessions                   : total number of completed TCP sessions
* rx_decoding_blocks            : total number of blocks successfully decoded
* rx_decoding_blocks_err        : total number of blocks lost due to decoding error: too many packets missing or corrupted at the time of decoding.
* rx_udp_pkts                   : total number of UDP packets successfully received 
* rx_udp_bytes                  : total number of bytes successfully received from UDP packets
* rx_udp_deserialize_header_err : total number of lost UDP packets due to corrupted header
* rx_udp_recv_pkts_err          : total number of read socket failure
* rx_udp_send_reorder_err       : total number of lost UDP packets because it was impossible to push it to the reorder/decode queue.  Try to increase "udp_packets_queue_size" receiver config value or reduce throughput with rate limiter or try to optimize RX performance receiver :ref:`multithreading`.
* rx_udp_pkts_missing           : total number of missing UDP packets when trying to decode blocks (packet drops, header error or queue full...).
* rx_tcp_blocks                 : total number of blocks sent on TCP session
* rx_tcp_blocks_err             : total number of lost blocks, not sent on TCP session (socket error)
* rx_tcp_bytes                  : total number of bytes sent on TCP session
* rx_tcp_bytes_err              : total number of lost bytes, not sent on TCP session (socket error)
* rx_pop_ok_packets             : total number of packets sent to reordering module and which completed blocks. Reordering module used this packet to complete a block and returns it. This value should be equal or inferior to rx_decoding_blocks. (Inferior because we can sometimes successfully decode a block even if we do not have all packets (see rx_pop_timeout_with_packets).
* rx_pop_ok_none                : total number of packets sent to reordering module, without finishing a block. Reordering module kept this packet and returned nothing, waiting for other packets to finish a block
* rx_pop_timeout_with_packets   : the current block did not receive the needed packets to complete it before a timeout occurs. We will try to decode the block and maybe succeed if we received enough data.
* rx_pop_timeout_none           : a timeout happens when there was no waiting packet for the current block.
* rx_send_block_err             : total number of lost blocks because it was impossible to push it to the TCP sender queue (most probably because it is full). Try to increase "tcp_blocks_queue_size" receiver config value or adjust sender/receiver TCP throughput.
* rx_skip_block                 : number of completed blocks dropped because the session is broken (we lost a previous block).

Summary of data loss metrics (diode-receive side)
-------------------------------------------------

Packet loss metrics
"""""""""""""""""""

If too many packets are lost, we will see block decoding error.

 * rx_udp_deserialize_header_err
 * rx_udp_send_reorder_err
 * rx_udp_pkts_missing
 * rx_udp_recv_pkts_err (maybe ? not sure of possible error case)


Block loss metrics
""""""""""""""""""

If a block is lost, the whole session is lost.

 * rx_decoding_blocks_err
 * rx_send_block_err
 * rx_tcp_blocks_err

