Exchanging files with Lidi
==========================

There are 3 applications with lidi which are used to test or for a first setup.
Two of them can be used to send packets, the last one is used to receive packets.

These application implements a simple protocol to be able to send multiple files in the same session.
Files on receiver side will be recreated with their original name and their unix metadata.

They help to understand how to build your own client/server application.

Sending files
"""""""""""""

To send files, there is an application which can be used in "one-shot" mode, sending all files provided in command line as fast as possible, then disconnects.

.. code-block::

   Usage: diode-send-file [OPTIONS] [FILE]
   
   Arguments:
     [FILE]...  List of files to send
   
   Options:
         --to-tcp <TO_TCP>         IP address and port to connect in TCP to diode-send (ex "127.0.0.1:5001") [default: 127.0.0.1:5001]
         --buffer-size <nb_bytes>  Size of file buffer [default: 8196]
         --hash                    Compute a hash of file content (default is false)
         --log-config <file>       Path to log configuration file
         --debug                   Verbosity level. Using it multiple times adds more logs
         --help                    Print help
         --version                 Print version

Another application is here to watch for changes in a given directory and send files them as they come : diode-send-dir

.. code-block::

   Usage: diode-send-dir [OPTIONS] <DIR>
   
   Options:
         --to-tcp <TO_TCP>                IP address and port to connect in TCP to diode-send (ex "127.0.0.1:5001") [default: 127.0.0.1:5001]
         --buffer-size <BUFFER_SIZE>      Size of file buffer [default: 8196]
         --hash                           Compute a hash of file content (default is false)
         --dir <DIR>                      Directory containing files to send
         --maximum-files <MAXIMUM_FILES>  maximum number of files to send per session
         --log-config <LOG_CONFIG>        Path to log configuration file
         --debug...                       Verbosity level. Using it multiple times adds more logs
         --help                           Print help
         --version                        Print version

Receiving files
"""""""""""""""

A single application is used to receive files in any case. It will create files in the provided directory. It will fail if a file with the same name already exists.

.. code-block::

   Usage: diode-receive-file [OPTIONS] <DIR>
   
   Arguments:
     <DIR>  Output directory
   
     Options:
           --bind-tcp <BIND_TCP>        IP address and port to accept TCP connections from diode-receive (default 127.0.0.1:5002) [default: 127.0.0.1:5002]
           --buffer-size <BUFFER_SIZE>  Size of file buffer [default: 8196]
           --hash                       Verify the hash of file content (default is false)
           --log-config <LOG_CONFIG>    Path to log configuration file
           --debug...                   Verbosity level. Using it multiple times adds more logs
           --help                       Print help
           --version                    Print version

TO BE TESTED: what happen if transfer is incomplete ???

