Sending files with Lidi
=======================

.. code-block::

   Usage: diode-send-file [OPTIONS] <--to_tcp <ip:port>|--to_unix <path>> <file>...
   
   Arguments:
     <file>...
   
   Options:
         --to_tcp <ip:port>        IP address and port to connect in TCP to diode-send
         --to_unix <path>          Path of Unix socket to connect to diode-send
         --buffer_size <nb_bytes>  Size of file read/client write buffer [default: 4194304]
         --hash                    Compute a hash of file content (default is false)
     -h, --help                    Print help
     -V, --version                 Print version

.. code-block::

   Usage: diode-receive-file [OPTIONS] [dir]
   
   Arguments:
     [dir]  Output directory [default: .]
   
   Options:
         --from_tcp <ip:port>      IP address and port to accept TCP connections from diode-receive [default: 127.0.0.1:7000]
         --from_unix <path>        Path of Unix socket to accept Unix connections from diode-receive
         --buffer_size <nb_bytes>  Size of client write buffer [default: 4194304]
         --hash                    Verify the hash of file content (default is false)
     -h, --help                    Print help
     -V, --version                 Print version
