# implementation of steps for network behavior

from behave import given, when, use_step_matcher

def string_to_bytes(size):
    count = int(size[0:-2])
    blocksize = size[-2:]

    if blocksize == 'KB':
        count = count * 1024
    elif blocksize == 'MB':
        count = count * 1024 * 1024
    elif blocksize == 'GB':
        count = count * 1024 * 1024 * 1024
    else:
        raise Exception("Unknown unit")

    return count

use_step_matcher("cfparse")

@given('there is a network interrupt of {network_up_after} after {network_down_after}')
def step_impl(context, network_up_after, network_down_after):
    context.network_up_after = string_to_bytes(network_up_after)
    context.network_down_after = string_to_bytes(network_down_after)

@given('there is a network drop of {percent} %')
def step_given_network_drop_percent(context, percent):
    context.network_drop = percent

@given('there is a limited network bandwidth of {bandwidth} Mb/s')
def step_given_network_limited_bandwidth(context, bandwidth):
    context.network_max_bandwidth = int(bandwidth) * 1000000
