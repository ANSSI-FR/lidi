Oneshot Lidi
============

.. code-block:: none

   Read stdin and send it to diode-oneshot-receive (no need for diode-send nor diode-receive).

   Usage: diode-oneshot-send [OPTIONS] --to <ip:port>

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --encode-threads <0..255>
             Number of parallel RaptorQ encoding threads [default: 1]
         --flush
             Flush client data immediately
         --to <ip:port>
             IP address and port where to send UDP packets to diode-receive
         --to-bind <ip:port>
             Binding IP for UDP traffic [default: 0.0.0.0:0]
         --to-mtu <nb_bytes>
             MTU of the output UDP link [default: 1500]
         --batch <2..1024>
             Use sendmmsg to send from 2 to 1024 UDP datagrams at once
         --block <nb_bytes>
             Size of RaptorQ block in bytes [default: 734928]
         --repair <percentage>
             Percentage of RaptorQ repair data [default: 2]
         --cpu-affinity
             Set CPU affinity for threads
     -h, --help
             Print help

.. code-block:: none

   Receive data from diode-oneshot-send and write them to stdout (no need for diode-send nor diode-receive).

   Usage: diode-oneshot-receive [OPTIONS] --from <ip:port>

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --from <ip:port>
             IP address and port where to receive UDP packets from diode-send
         --from-mtu <nb_bytes>
             MTU of the input UDP link [default: 1500]
         --batch <2..1024>
             Use recvmmsg to receive from 2 to 1024 UDP datagrams at once
         --reset-timeout <seconds>
             Reset diode if no data are received after duration [default: 2]
         --decode-threads <0..255>
             Number of parallel RaptorQ decode threads [default: 1]
         --flush
             Flush immediately data to clients
         --abort-timeout <seconds>
             Abort connections if no data received after duration (0 = no abort)
         --block <nb_bytes>
             Size of RaptorQ block in bytes [default: 734928]
         --repair <percentage>
             Percentage of RaptorQ repair data [default: 2]
         --cpu-affinity
             Set CPU affinity for threads
     -h, --help
             Print help
