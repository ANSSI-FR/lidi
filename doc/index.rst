Welcome to Lidi's documentation!
================================

What is lidi?
-------------

Lidi (leedee) allows you to copy TCP streams (or files) over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: `Wikipedia - Unidirectional network <https://en.wikipedia.org/wiki/Unidirectional_network>`_.

Why lidi?
---------

Lidi has been developed to answer a specific need: copy TCP streams (or files) across a unidirectional link fast and reliably.

Lidi was designed from the ground up to achieve these goals, for example the Rust language has been chosen for its strong safety properties as well as its very good performance profile.

Caveat
------

If you want to run lidi closer to its intended speed, tuning :ref:`Command line parameters` according to your network configuration is certainly required.
Read the :ref:`Tweaking parameters` section for details.

.. toctree::
   :maxdepth: 2
   :caption: Contents:

   gstarted
   parameters
   tweaking
   files 


Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`
