import subprocess

class TcUdpShaper:
    """
    Limite le débit UDP sortant sur loopback via tc HTB + filtre u32.
    Cible : trafic UDP à destination du port donné sur 127.0.0.1.
    Nécessite CAP_NET_ADMIN (root ou sudo).
    """

    IFACE = "lo"

    def __init__(self, rate: str, port: int = 5000):
        """
        rate : notation tc, ex. "10mbit", "500kbit"
        port : port UDP destination à limiter (lidi-send → lidi-receive)
        """
        self.rate = rate
        self.port = port

    def _run(self, *args, check=True):
        subprocess.run(["tc", *args], check=check, capture_output=True)

    def setup(self):
        """Installe les règles tc sur lo."""
        self._teardown_silent()

        # qdisc racine HTB
        self._run("qdisc", "add", "dev", self.IFACE, "root",
                  "handle", "1:", "htb", "default", "99")

        # Classe par défaut : pas de limite (trafic non ciblé passe librement)
        self._run("class", "add", "dev", self.IFACE,
                  "parent", "1:", "classid", "1:99",
                  "htb", "rate", "1gbit")

        # Classe limitée pour l'UDP ciblé
        self._run("class", "add", "dev", self.IFACE,
                  "parent", "1:", "classid", "1:10",
                  "htb", "rate", self.rate, "burst", "32kbit")

        # Filtre u32 : UDP (proto 17) à destination du port cible
        self._run("filter", "add", "dev", self.IFACE,
                  "parent", "1:", "protocol", "ip",
                  "prio", "1", "u32",
                  "match", "ip", "protocol", "17", "0xff",   # UDP
                  "match", "ip", "dport", str(self.port), "0xffff",
                  "flowid", "1:10")

    def teardown(self):
        """Supprime les règles tc."""
        self._teardown_silent()

    def _teardown_silent(self):
        self._run("qdisc", "del", "dev", self.IFACE, "root", check=False)