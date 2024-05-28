Performance and fiability parameters
====================================

To be transferred through the diode, data is sliced by lidi at different levels:

 - into `blocks` at the logical fountain codes level,
 - into `packets` at the UDP transfer level.

One can have effect on the slicing sizes to achieve optimal performances by using several command line options.

Packet sizes
------------

Firstly, the parameter which has the biggest impact on the network is the packet size.
If possible, MTU on the UDP interface should be increased, and must be set to the same value on sender and receiver sides:

.. code-block::

   --udp-mtu <nb_bytes>

Default MTU is set to 1500 (default MTU on ethernet interfaces) and should be increased. A higher value will reduce a lot the number of packets to manage in the kernel.
Of course, this number should not exeed network interface parameter or packet fragmentation will occur before sending the packet and the benefits of this parameter will be lost.

Try to adjust to 9000 if possible on the network, for example:

.. code-block::

   ip link set dev <myinterface> mtu 9000
   cargo run --release --bin diode-send -- --udp-mtu 9000 ...
   cargo run --release --bin diode-receive -- --udp-mtu 9000 ...

Block sizes
-----------

Then, on the logical level, fountain code operate on blocks. Blocks have fixed size and will be split in IP packets to be sent on the network. 
Blocks are made of two parts : encoding block and repair block. Encoding block contains original data. Repair block is optional and represents redundancy : they are used by fountain codes to ensure data reconstruction.

On both sides, parameters have the same name and must be set to the same values.

.. code-block::

   --encoding-block-size <nb_bytes>
  
   --repair-block-size <nb_bytes>

The default value for an encoding block is 60000 and repair block size is 6000 (10% of encoding block value). This mean we have 10% of data overhead on all transfers. But this allows to have small packet loss or corruption and still being able to reconstruct the original block.

To prevent more overhead when mapping blocks on packets, encoding block and repair block must match a factor of the defined UDP MTU. The exact algorithm is : defined mtu - ip header size (20) - udp header size (8) - raptor header size (4) - lidi protocol header size (4).

See the :ref:`Tweaking parameters` chapter for more details on how to choose optimal values for your particular use case and devices.

Rate limiting
-------------

Basically, since lidi diode-send scales pretty well and can reach very high throughput, it is often necessary to ratelimit speed of diode-send to prevent packet drop in network.
It is possible to use to following parameter to limit transmission speed :

.. code-block::

   --max-bandwidth <bit/s>

To make it simple, rate limiting is applied at TCP receive level. At this stage, we don't know the overhead introduced by repair blocks and UDP packet's headers. That means this value should be lower than the output interface speed, including the overhead introduced by repair blocks and UDP packets.

Multithreading
--------------

Lidi is designed to reach up to 10Gb/s on an actual x86 CPU with multiple cores.
Sending, receiving, encoding and decoding packets are CPU intensive operations.

On sender side, if receiving data from local TCP socket is really fast, encoding and sending packets is quite slow. On a modern x86, each core can encode and send up to 3 Gb/s of data. To reach up to 10 Gb/s throughput, it is mandatory to use multiple threads for this operation.

.. code-block::

   --nb-threads <nb>

Default value is 4. That means diode-send will use 5 cores, 1 for TCP receive and 4 to encode and send UDP. It should not be necessary to change this value, except on low frequency CPU.

On receiver side, there are 3 steps and UDP packet receive thread seems to be the most CPU intensive. It is possible to increase the number of threads receiving packets using this parameter:

.. code-block::

   --nb-threads <nb>

Default value is 1. It should not be necessary to change this value, especially when using an increased MTU.

.. _Tweaking parameters:


Kernel parameters
-----------------

If you want to run lidi closer to its intended speed, please set the following sysctl to the maximum value (root required):

Mandatory parameter:

.. code-block::

   net.core.rmem_max=2000000000

Optional parameters (to be checked):

.. code-block::

   net.core.wmem_max=67108864
   net.core.netdev_max_backlog=1000
   net.ipv4.udp_mem="12148128 16197504 67108864"
