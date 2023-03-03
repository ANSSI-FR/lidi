Sending files with Lidi
=======================

.. code-block::

   Usage: diode-send-file [OPTIONS] <file>...
   
   Arguments:
     <file>...
   
   Options:
         --to_tcp <ip:port>        Address and port to connect to diode-send [default: 127.0.0.1:5000]
         --buffer_size <nb_bytes>  Size of file read/TCP write buffer [default: 4194304]
     -h, --help                    Print help
     -V, --version                 Print version

.. code-block::

   Usage: diode-receive-file [OPTIONS] [dir]
   
   Arguments:
     [dir]  Output directory [default: .]
   
   Options:
         --from_tcp <ip:port>      Address and port to listen for diode-receive [default: 127.0.0.1:7000]
         --buffer_size <nb_bytes>  Size of TCP write buffer [default: 4194304]
     -h, --help                    Print help
     -V, --version                 Print version
