.. _Tweaking parameters:

Tweaking parameters
===================

If you want to run lidi closer to its intended speed, please set the following sysctl (root required):

.. code-block::

   net.core.rmem_max=67108864
   net.core.wmem_max=67108864
   net.core.netdev_max_backlog=1000
   net.ipv4.udp_mem="12148128 16197504 67108864"
