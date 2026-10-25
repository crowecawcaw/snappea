"""Contract-based histories; kept outside solver containers."""

def event(name, **payload):
    return {'event': name, 'payload': payload, 'key': 'lease', 'timestamp': 0}


def cases():
    e = event
    acquire = lambda c,t,k: e('eAcquire',client=c,ttl=t,token=k)
    write = lambda c,k,v,ok: e('eWrite',client=c,token=k,value=v,accepted=ok)
    release = lambda c,k,ok: e('eRelease',client=c,token=k,accepted=ok)
    advance = lambda n: e('eAdvance',ticks=n)
    read = lambda v: e('eRead',value=v)
    peek = lambda c,k,x: e('ePeek',owner=c,token=k,expires_at=x)
    # Same valid prefix, one altered response/state per negative history.
    tests = [
        ('valid_initial', True, [read(0),peek('-',-1,-1)]),
        ('valid_owner_and_rejections', True, [acquire('maya',3,4),acquire('maya',2,-1),
            acquire('noor',2,-1),write('noor',4,31,False),write('maya',3,31,False),
            write('maya',4,31,True),read(31),release('noor',4,False),peek('maya',4,3)]),
        ('valid_expiry_successor', True, [acquire('maya',2,7),write('maya',7,13,True),
            advance(2),peek('-',-1,-1),write('maya',7,17,False),acquire('noor',4,11),
            release('maya',7,False),write('maya',7,99,False),write('noor',11,23,True),
            read(23),peek('noor',11,6),release('noor',11,True),peek('-',-1,-1)]),
        ('valid_release_reacquire', True, [acquire('maya',2,1),release('maya',1,True),
            acquire('maya',2,2),write('maya',1,4,False),write('maya',2,9,True),read(9)]),
        ('valid_zero_advance_and_later_expiry', True, [advance(0),acquire('noor',5,3),advance(4),
            peek('noor',3,5),advance(2),release('noor',3,False),read(0)]),
        ('reject_two_owners', False,[acquire('maya',4,1),acquire('noor',2,2)]),
        ('reject_owner_reacquire',False,[acquire('maya',4,1),acquire('maya',2,2)]),
        ('reject_available_denial',False,[acquire('maya',2,-1)]),
        ('reject_wrong_owner_write',False,[acquire('maya',4,1),write('noor',1,3,True)]),
        ('reject_wrong_token_write',False,[acquire('maya',4,1),write('maya',2,3,True)]),
        ('reject_expired_write',False,[acquire('maya',2,1),advance(2),write('maya',1,3,True)]),
        ('reject_stale_release',False,[acquire('maya',2,1),advance(2),acquire('noor',4,2),release('maya',1,True)]),
        ('reject_token_reuse',False,[acquire('maya',2,5),release('maya',5,True),acquire('maya',2,5)]),
        ('reject_missing_effect',False,[acquire('maya',2,1),write('maya',1,19,True),read(0)]),
        ('reject_rejected_write_effect',False,[acquire('maya',2,1),write('noor',1,19,False),read(19)]),
        ('reject_release_clears_value',False,[acquire('maya',2,1),write('maya',1,19,True),release('maya',1,True),read(0)]),
        ('reject_wrong_expiry',False,[acquire('maya',3,1),peek('maya',1,4)]),
        ('reject_missing_owner',False,[acquire('maya',3,1),peek('-',-1,-1)]),
    ]
    for name, valid, events in tests:
        for i, record in enumerate(events):
            record['timestamp'] = i
        yield name, valid, events
