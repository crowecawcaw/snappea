event Start;
event Commit;
event Ping: machine;
event Pong: bool;
event Tick;
// A write completion races a reader doing repeated request/response rounds.
machine Store  {
    var committed: bool;
    start state Ready  {
        on Commit do  {
            committed = true;
        }
        on Ping do (reader: machine)  {
            send reader, Pong, committed;
        }
    }
}
machine Writer  {
    var store: machine;
    start state Ready  {
        entry (s: machine)  {
            store = s;
        }
        on Start do  {
            send store, Commit;
        }
    }
}
machine Reader  {
    var store: machine;
    var rounds: int;
    var sawCommit: bool;
    var mode: int;
    var goal: int;
    start state Ready  {
        entry (s: machine)  {
            store = s;
            mode=choose(3);
            goal=128 + mode * 8;
        }
        on Start do  {
            send store, Ping, this;
        }
        on Pong do (committed: bool)  {
            sawCommit = sawCommit || committed;
            rounds = rounds + 1;
            if (rounds == goal)  {
                if (mode == 0)  {
                    assert sawCommit, "stall-0";
                }
                if (mode == 1)  {
                    assert sawCommit, "stall-1";
                }
                if (mode == 2)  {
                    assert sawCommit, "stall-2";
                }
            }
            else  {
                send store, Ping, this;
            }
        }
    }
}
// Optional background work prevents the finite state search from trivially closing.
machine Maintenance  {
    var ticks: int;
    start state Ready  {
        entry  {
            send this, Tick;
        }
        on Tick do  {
            ticks = ticks + 1;
            send this, Tick;
        }
    }
}
machine Main  {
    start state Init  {
        entry  {
            var store: machine;
            var writer: machine;
            var reader: machine;
            store = new Store();
            reader = new Reader(store);
            writer = new Writer(store);
            new Maintenance();
            send writer, Start;
            send reader, Start;
        }
    }
}
test tcMain [main=Main]:  {
    Main, Store, Writer, Reader, Maintenance
}
;
