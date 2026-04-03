Rate limiting with tc
=====================

Lidi no longer includes an internal bandwidth limiter. Instead, network traffic shaping is handled by the Linux `tc` (traffic control) utility, which provides more precise and flexible control over network throughput.

Why use tc for rate limiting?
-----------------------------

Using `tc` provides:

- Accurate control of actual network throughput
- Support for complex network topologies
- Better handling of packet overheads
- Integration with kernel networking stack
- More granular control per interface and protocol

Prerequisites
-------------

To use `tc`, your user needs the `CAP_NET_ADMIN` capability. There are several ways to configure this:

**Option 1: Add to netdev group** (recommended)

.. code-block:: bash

   sudo usermod -aG netdev $USER

Log out and log back in for changes to take effect.

**Option 2: Add capabilities to tc binary**

.. code-block:: bash

   sudo setcap cap_net_admin+ep /sbin/tc

**Option 3: Use sudo**

Run tc commands with sudo or configure sudoers to allow tc without password.

Basic tc configuration for Lidi
--------------------------------

Lidi typically sends UDP traffic on a specific port (default: 5000) to a specific destination IP. Here is a basic tc configuration:

Setup rate limiting
~~~~~~~~~~~~~~~~~~~

.. code-block:: bash

   # Create HTB qdisc on loopback interface
   sudo tc qdisc add dev lo root handle 1: htb default 99

   # Create default class (unlimited traffic)
   sudo tc class add dev lo parent 1: classid 1:99 htb rate 1gbit

   # Create limited class for Lidi UDP traffic
   sudo tc class add dev lo parent 1: classid 1:10 htb rate 100mbit burst 32kbit

   # Filter UDP traffic to Lidi port
   sudo tc filter add dev lo parent 1: protocol ip prio 1 u32 \
     match ip protocol 17 0xff \
     match ip dport 5000 0xffff \
     flowid 1:10

Replace ``100mbit`` with your desired bandwidth limit and ``5000`` with your Lidi UDP port.

Teardown
~~~~~~~~

.. code-block:: bash

   sudo tc qdisc del dev lo root

Advanced configuration
----------------------

Multiple ports
~~~~~~~~~~~~~~

If you use multiple UDP ports (for multithreading), add multiple filters:

.. code-block:: bash

   sudo tc filter add dev lo parent 1: protocol ip prio 1 u32 \
     match ip protocol 17 0xff \
     match ip dport 5000 0xffff \
     flowid 1:10

   sudo tc filter add dev lo parent 1: protocol ip prio 1 u32 \
     match ip protocol 17 0xff \
     match ip dport 5001 0xffff \
     flowid 1:10

Different rates per port
~~~~~~~~~~~~~~~~~~~~~~~~

You can create separate classes for different ports with different rates:

.. code-block:: bash

   # Class for port 5000 at 100mbit
   sudo tc class add dev lo parent 1: classid 1:10 htb rate 100mbit

   # Class for port 5001 at 50mbit
   sudo tc class add dev lo parent 1: classid 1:20 htb rate 50mbit

   # Filters
   sudo tc filter add dev lo parent 1: protocol ip prio 1 u32 \
     match ip protocol 17 0xff \
     match ip dport 5000 0xffff \
     flowid 1:10

   sudo tc filter add dev lo parent 1: protocol ip prio 1 u32 \
     match ip protocol 17 0xff \
     match ip dport 5001 0xffff \
     flowid 1:20

Monitoring tc rules
-------------------

View current tc configuration:

.. code-block:: bash

   sudo tc qdisc show dev lo
   sudo tc class show dev lo
   sudo tc filter show dev lo

View statistics:

.. code-block:: bash

   sudo tc -s qdisc show dev lo

Integration with Lidi tests
---------------------------

The test suite includes a ``TcUdpShaper`` class in ``features/steps/tc_shaper.py`` that automates tc configuration for testing. This class:

- Sets up HTB qdisc with filtered UDP rate limiting
- Targets specific UDP destination ports
- Provides setup() and teardown() methods for test fixtures
- Requires CAP_NET_ADMIN capability

Example usage in tests:

.. code-block:: python

   from features.steps.tc_shaper import TcUdpShaper

   shaper = TcUdpShaper(rate="100mbit", port=5000)
   shaper.setup()
   # Run tests...
   shaper.teardown()

Troubleshooting
---------------

Check if tc is available:

.. code-block:: bash

   which tc

Check if you have CAP_NET_ADMIN:

.. code-block:: bash

   capsh --print

Common issues:

- ``Operation not permitted``: User lacks CAP_NET_ADMIN capability
- ``File exists``: qdisc already exists, remove it first with ``tc qdisc del``
- No effect: Verify you are shaping the correct interface (lo for local, eth0 for network)
