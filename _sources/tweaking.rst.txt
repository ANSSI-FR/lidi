.. _Tweaking parameters:

Tweaking parameters
===================

On the receiver side some kernel/hardware parameters need to be adjusted to ensure no packets are lost. Use `ethtool -g` to read available hardware configuration. On the example below, RX ring parameter for `eth0` is not set (line `RX: n/a` for `Current hardware settings`) while it can be set to `4096` (line `RX: 4096` for `Pre-set maximum`).

.. code-block:: none

   root@diode:~# ethtool -g eth0
   Ring parameters for eth0:
   Pre-set maximums:
   RX:			4096
   RX Mini:		n/a
   RX Jumbo:		n/a
   TX:			n/a
   TX push buff len:	n/a
   Current hardware settings:
   RX:			n/a
   RX Mini:		n/a
   RX Jumbo:		n/a
   TX:			n/a
   RX Buf Len:		n/a
   CQE Size:		n/a
   TX Push:		off
   RX Push:		off
   TX push buff len:	n/a
   TCP data split:	n/a

Set the RX ring parameter to the maximum value supported by the card. On the example above, simply run `ethtool -G eth0 rx 4096` and then `ethtool -g eth0` to check the RX ring parameter has been set correctly (see below).

.. code-block:: none

   root@diode:~# ethtool -G eth0 rx 4096
   root@diode:~# ethtool -g eth0
   Ring parameters for eth0:
   Pre-set maximums:
   RX:			4096
   RX Mini:		n/a
   RX Jumbo:		n/a
   TX:			n/a
   TX push buff len:	n/a
   Current hardware settings:
   RX:			4096
   RX Mini:		n/a
   RX Jumbo:		n/a
   TX:			n/a
   RX Buf Len:		n/a
   CQE Size:		n/a
   TX Push:		off
   RX Push:		off
   TX push buff len:	n/a
   TCP data split:	n/a


Some sysctl values need to be set (as root) on the receiver side to ensure kernel buffers are large enough to ensure no packet are lost. The value `97536000` is adequate for lidi defaults parameters. This value can be computed by multiplying the MTU on the UDP link (default is `1500`) by the number of packets per block (default is `512`), multiplied by `127` (constant value to ensure lidi can bufferize 127 blocks).

.. code-block:: none

   sysctl -w net.ipv4.udp_mem="97536000 97536000 97536000"
   sysctl -w net.core.rmem_max=97536000
   sysctl -w net.ipv4.udp_rmem_min=97536000

You can also set respective parameters on the sender side, but this is only required to achieve high throughput (above 2.5Gb/s).

.. code-block:: none

   sysctl -w net.ipv4.udp_mem="97536000 97536000 97536000"
   sysctl -w net.core.wmem_max=97536000
   sysctl -w net.ipv4.udp_wmem_min=97536000


