from contextlib import contextmanager
import os

def build_lidi_config(context, udp_port, log_config):
    """Build LIDI configuration string based on context and parameters."""
    # Use values from tcp.config.toml example as base
    mtu = getattr(context, 'mtu', 1500) or 1500
    ports = [int(udp_port)]
    block = getattr(context, 'block_size', 20_000) or 20_000
    repair = getattr(context, 'repair', 1) or 1
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
        f"heartbeat = {heartbeat}",
        "",
        "[send]",
        'log = "INFO"',
        'to = "127.0.0.1"',
        'to_bind = "0.0.0.0:0"',
        'mode = "mmsg"',
        'prometheus_listen = "127.0.0.1:9001"',
        f"{log_config}",
        f'from = [ "tcp[hash={str(hash_val).lower()},flush={str(flush).lower()}]:127.0.0.1:{context.tcp_send_port}" ]',
        "",
        "[receive]",
        'log = "INFO"',
        'from = "127.0.0.1"',
        'mode = "mmsg"',
        "queue_size = 4096",
        "reset_timeout = 2",
        "abort_timeout = 60",
        'prometheus_listen = "127.0.0.1:9002"',
        f"{log_config}",
        f'to = [ "tcp[hash={str(hash_val).lower()},flush={str(flush).lower()}]:127.0.0.1:{context.tcp_receive_port}" ]'
    ]

    return "\n".join(config_lines)

def write_lidi_config(context, filename, udp_port, log_config):
    """Write LIDI configuration to file."""
    full_path = os.path.join(context.base_dir, filename)
    log_config_str = f"log4rs_config = \"{log_config}\""
    with open(full_path, "w") as config_file:
        config_file.write(build_lidi_config(context, udp_port, log_config_str))
    return full_path

def build_lidi_send_command(context):
    lidi_config = write_lidi_config(context, "lidi_send.toml", "5000", context.log_config_lidi_send)

    lidi_send_command = [f'{context.bin_dir}/lidi-send', lidi_config]

    return lidi_send_command

def build_lidi_receive_command(context):
    # Determine UDP port based on network behavior
    has_network_simulator = (
        context.network_down_after or
        context.network_up_after or
        context.network_drop or
        context.network_max_bandwidth or
        context.bandwidth_must_not_exceed
    )
    receiver_bind_udp_port = "6000" if has_network_simulator else "5000"

    lidi_config = write_lidi_config(context, "lidi_receive.toml", receiver_bind_udp_port, context.log_config_lidi_receive)

    lidi_receive_command = [f'{context.bin_dir}/lidi-receive', lidi_config]

    return lidi_receive_command

def build_lidi_receive_file_command(context):
    lidi_receive_file_command = [
        f'{context.bin_dir}/lidi-file-receive',
        '--from-tcp',
        f'127.0.0.1:{context.tcp_receive_port}',
        '--log-config', context.log_config_lidi_receive_file,
        context.receive_dir
    ]

    return lidi_receive_file_command

def build_lidi_send_dir_command(context, watch, ignore):
    lidi_send_dir_command = [
        f'{context.bin_dir}/lidi-dir-send',
        '--to-tcp', f'127.0.0.1:{context.tcp_send_port}',
        '--log-config', context.log_config_lidi_send_dir
    ]

    if watch:
        lidi_send_dir_command += ['--watch']
    
    if ignore is not None:
        lidi_send_dir_command += ['--ignore', ignore]
        
    lidi_send_dir_command += [context.send_dir]
    
    return lidi_send_dir_command

def build_lidi_send_file_command(context, filename):
    # Création de la liste de base pour la commande
    base_command = [
        f"{context.bin_dir}/lidi-file-send",
        "--buffer-size",
        "8192",
        "--to-tcp",
        f"127.0.0.1:{context.tcp_send_port}",
        '--log-config', context.log_config_lidi_send_file
    ]
    
    # Convertir filename en liste pour la fusion
    # Si filename est déjà une liste, l'utiliser telle quelle
    # Sinon, le mettre dans une liste
    if isinstance(filename, list):
        filename_list = filename
    else:
        filename_list = [filename]
    
    # Fusion des deux listes : la commande de base et la liste contenant filename
    lidi_send_file_command = base_command + filename_list
    
    return lidi_send_file_command

def build_network_simulator_command(context):
    # Setup network behavior parameters
    network_simulator_command = [
        f'{context.bin_dir}/lidi-network-simulator',
        '--bind-udp', '0.0.0.0:5000',
        '--to-udp', '127.0.0.1:6000',
        '--log-config', context.log_config_network_behavior
    ]
    
    # Add network behavior options
    network_options = [
        ('network_down_after', '--network-down-after'),
        ('network_up_after', '--network-up-after'),
        ('network_drop', '--loss-rate'),
        ('network_max_bandwidth', '--max-bandwidth'),
        ('bandwidth_must_not_exceed', '--abort-on-max-bandwidth')
    ]
    
    use_network_simulator = False
    for attr_name, option in network_options:
        attr_value = getattr(context, attr_name, None)
        if attr_value:
            network_simulator_command.extend([option, str(attr_value)])
            use_network_simulator = True

    if not use_network_simulator:
        return None
    else:
        return network_simulator_command
