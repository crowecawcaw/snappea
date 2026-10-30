event Hello: int;
machine Server  {
    var got: seq[int];
    start state Init  {
        on Hello do (x: int)  {
            got += (sizeof(got), x);
            if (sizeof(got) == 6)  {
                if (got[0] == 4 && got[1] == 1 && got[2] == 5 && got[3] == 0 && got[4] == 3 && got[5] == 2)  {
                    assert false, "order-0";
                }
                if (got[0] == 2 && got[1] == 5 && got[2] == 1 && got[3] == 3 && got[4] == 0 && got[5] == 4)  {
                    assert false, "order-1";
                }
                if (got[0] == 3 && got[1] == 0 && got[2] == 4 && got[3] == 2 && got[4] == 5 && got[5] == 1)  {
                    assert false, "order-2";
                }
            }
        }
    }
}
machine Client  {
    start state Init  {
        entry (p: (s: machine, x: int))  {
            send p.s, Hello, p.x;
        }
    }
}
machine Main  {
    start state Init  {
        entry  {
            var s: machine;
            s = new Server();
            new Client((s=s, x=0));
            new Client((s=s, x=1));
            new Client((s=s, x=2));
            new Client((s=s, x=3));
            new Client((s=s, x=4));
            new Client((s=s, x=5));
        }
    }
}
test tcMain [main=Main]:  {
    Main, Server, Client
}
;
