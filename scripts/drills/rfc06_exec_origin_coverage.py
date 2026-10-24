"""Structural checks for the native private EXEC-origin census (not header verification)."""

# The private V2 writer requests 256 named entries per list; totals remain complete.
MAX_LISTED = 256
# PALW_OBJECT_CHUNK_MAX_BYTES (100_000) * MAX_COUNT (16) - 69.
# Deliberately pinned: the cross-language test refuses native drift rather than
# silently adopting another carriage rule from a changed source checkout.
NATIVE_HEADER_CAP_BYTES = 1_599_931
FIELDS = {'cap_bytes', 'eligible', 'within_cap', 'max_bytes_seen', 'oversized',
          'unavailable', 'oversized_total', 'unavailable_total'}


def _u64(value, name):
    if type(value) is not int or not 0 <= value < 2**64:
        raise ValueError(name + ' must be an exact u64')
    return value


def _hash(value):
    if not isinstance(value, str) or len(value) != 128 or any(c not in '0123456789abcdef' for c in value):
        raise ValueError('coverage named row requires a canonical native Hash64')
    return value


def validate_census(census):
    """Refuse damaged/incomplete counters before an acceptance consumer uses zero totals.

    This checks the exact private writer shape, including truncated named lists.
    It does not establish that any historical header or signature is authentic.
    """
    if not isinstance(census, dict) or set(census) != FIELDS:
        raise ValueError('unknown native coverage census shape')
    for name in FIELDS - {'oversized', 'unavailable'}:
        _u64(census[name], name)
    cap = census['cap_bytes']
    if cap != NATIVE_HEADER_CAP_BYTES:
        raise ValueError('coverage header carriage cap differs from the pinned native V1 rule')
    if census['within_cap'] + census['oversized_total'] + census['unavailable_total'] != census['eligible']:
        raise ValueError('coverage counters do not partition the eligible claims')
    maximum = census['max_bytes_seen']
    if bool(census['oversized_total']) != (maximum > cap):
        raise ValueError('coverage maximum contradicts oversized count')
    if census['within_cap'] + census['oversized_total'] == 0 and maximum != 0:
        raise ValueError('coverage maximum names an unread header')
    seen = set()
    for name, width in (('oversized', 3), ('unavailable', 2)):
        rows = census[name]
        if not isinstance(rows, list) or len(rows) != min(census[name + '_total'], MAX_LISTED):
            raise ValueError('coverage named list does not match its complete total and writer cap')
        for row in rows:
            if not isinstance(row, list) or len(row) != width:
                raise ValueError('coverage named row has the wrong shape')
            claim, _ = _hash(row[0]), _hash(row[1])
            if claim in seen:
                raise ValueError('coverage names the same claim more than once')
            seen.add(claim)
            if name == 'oversized':
                size = _u64(row[2], 'oversized header bytes')
                if not cap < size <= maximum:
                    raise ValueError('coverage named oversized header contradicts its cap or maximum')
    return census
