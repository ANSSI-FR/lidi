.. _Logging:

Logging
=======

Log configuration
-----------------

By default, logs are displayed on console. But it is possible to configure logging system (for instance logging to a file) with a dedicated configuration file. Log system is log4rs, so you can use a configuration file compatible with this format.
Log system uses log4rs, so a typical configuration file "logconfig.yml" looks like:

.. code-block::

   # Scan this file for changes every 30 seconds
   refresh_rate: 30 seconds
   
   appenders:
     # An appender named "file" that writes to a file named lidi.log
     file:
       kind: file
       path: lidi.log
   
   # Set the default logging level to "warn" and attach the "stdout" appender to the root
   root:
     level: warn
     appenders:
       - file 

And we use this configuration using the following option (works on any application):

.. code-block::

   cargo run --release --bin <myapp> -- --log-config logconfig.yml ...

Verbosity
---------

It is possible to reduce log level filter with -d/--debug option. Each occurence of this option will reduce log level by one, printing more logs.

.. code-block::

   cargo run --release --bin <myapp> -- -ddd ...

Relevant logs
-------------

TODO
