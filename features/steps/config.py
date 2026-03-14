
from contextlib import contextmanager
import os

def build_lidi_config(context, udp_port, log_config):
    """Build LIDI configuration string based on context and parameters."""
    # Use values from tcp.config.toml example as base
    mtu = getattr(context, 'mtu', 9000)
    ports = [int(udp_port)]
    block = getattr(context, 'block_size', 300_000)
    repair = getattr(context, 'repair_block', 3)
    max_clients = 2
    hash_val = False
    flush = False
    heartbeat = 10

    # Base configuration similar to tcp.config.toml
    config_lines = [
        f"mtu = {mtu}",
        f"ports = {ports}",
        f"block = {block}",
        f"repair = {repair}",
        f"max_clients = {max_clients}",
        f"hash = {str(hash_val).lower()}",
        f"flush = {str(flush).lower()}",
        f"heartbeat = {heartbeat}",
        "",
        "[send]",
        'log = "DEBUG"',
        'to = "127.0.0.1"',
        'to_bind = "0.0.0.0:0"',
        'mode = "mmsg"',
        'prometheus_listen = "127.0.0.1:9001"',
        f"{log_config}",
        "",
        "[[send.from]]",
        f'tcp = "127.0.0.1:{context.tcp_send_port}"',
        "",
        "[receive]",
        'log = "DEBUG"',
        'from = "127.0.0.1"',
        'mode = "mmsg"',
        "queue_size = 4096",
        "reset_timeout = 2",
        "abort_timeout = 60",
        'prometheus_listen = "127.0.0.1:9002"',
        f"{log_config}",
        "",
        "[[receive.to]]",
        f'tcp = "127.0.0.1:{context.tcp_receive_port}"'
    ]

    return "\n".join(config_lines)

def write_lidi_config(context, filename, udp_port, log_config):
    """Write LIDI configuration to file."""
    full_path = os.path.join(context.base_dir, filename)
    log_config_str = f"log4rs_config = \"{log_config}\""
    with open(full_path, "w") as config_file:
        config_file.write(build_lidi_config(context, udp_port, log_config_str))
    return full_path

@contextmanager
def log_files(base_dir, name):
    """Context manager for handling log files."""
    log_file_path = os.path.join(base_dir, f'{name}.log')
    log_file_error_path = os.path.join(base_dir, f'{name}-error.log')
    
    with open(log_file_path, 'w') as stdout, open(log_file_error_path, 'w') as stderr:
        yield stdout, stderr
