"""Strict native wRPC getConnectedPeerInfo isolation check.

RPC core GetConnectedPeerInfoResponse serializes peer_info as peerInfo;
RpcPeerInfo.address is utils::networking::NetAddress {ip,port}, never a string.
This checks observations, not cryptographic identity or permanent isolation.
"""
import ipaddress

def require_loopback_peers_v1(response):
    if not isinstance(response,dict) or set(response)!={'peerInfo'} or not isinstance(response['peerInfo'],list):
        raise ValueError('unknown native peer inventory schema; expected peerInfo list')
    peers=[]
    for peer in response['peerInfo']:
        if not isinstance(peer,dict):raise ValueError('native peer record is not an object')
        address=peer.get('address')
        if not isinstance(address,dict) or set(address)!={'ip','port'}:
            raise ValueError('native peer address must contain exact ip and port fields')
        host=address['ip'];port=address['port']
        if not isinstance(host,str) or '%' in host or type(port) is not int or not 1<=port<=65535:
            raise ValueError('invalid native peer IP or port')
        try:parsed=ipaddress.ip_address(host)
        except ValueError as error:raise ValueError('native peer IP is not a numeric address') from error
        mapped=getattr(parsed,'ipv4_mapped',None)
        if not parsed.is_loopback and not (mapped is not None and mapped.is_loopback):
            raise ValueError('non-loopback native peer detected')
        peers.append((str(parsed),port))
    return peers
