Sending files with Lidi
=======================

.. code-block:: none

  Usage: diode-send-file [OPTIONS] <--to-tcp <ip:port>|--to-unix <path>> [FILES]...

  Arguments:
    [FILES]...  Files to send

  Options:
        --log-level <Error|Warn|Info|Debug|Trace>
            Log level [default: Info]
        --to-tcp <ip:port>
            TCP address and port to connect to diode-send
        --to-unix <path>
            Path to Unix socket to connect to diode-send
        --buffer-size <bytes>
            Size of client internal read/write buffer [default: 4194304]
        --hash
            Compute and send the hash of file content
    -h, --help
            Print help

.. code-block:: none

   Usage: diode-receive-file [OPTIONS] <--from-tcp <ip:port>|--from-unix <path>> [OUTPUT_DIRECTORY]

   Arguments:
     [OUTPUT_DIRECTORY]  Output directory [default: .]

   Options:
         --log-level <Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --from-tcp <ip:port>
             IP address and port to accept TCP connections from diode-receive
         --from-unix <path>
             Path of Unix socket to accept Unix connections from diode-receive
         --buffer-size <bytes>
             Size of client write buffer [default: 4194304]
         --hash
             Verify the hash of file content
     -h, --help
             Print help

