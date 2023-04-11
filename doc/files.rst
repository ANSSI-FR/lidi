Sending files with Lidi
=======================

.. code-block::

   Usage: diode-send-file [OPTIONS] <file>...
   
   Arguments:
     <file>...
   
   Options:
         --to_tcp <ip:port>        IP address and port to connect in TCP to diode-send
         --to_unix <path>          Path of Unix socket to connect to diode-send
         --buffer_size <nb_bytes>  Size of file read/client write buffer [default: 4194304]
     -h, --help                    Print help
     -V, --version                 Print version

.. code-block::

   Usage: diode-receive-file [OPTIONS] [dir]
   
   Arguments:
     [dir]  Output directory [default: .]
   
   Options:
         --from_tcp <ip:port>      IP address and port to listen for TCP connections from diode-receive [default: 127.0.0.1:7000]
         --from_unix <path>        Path to listen for Unix connections from diode-receive
         --buffer_size <nb_bytes>  Size of client write buffer [default: 4194304]
     -h, --help                    Print help
     -V, --version                 Print version
