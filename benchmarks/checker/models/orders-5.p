event Hello: int;
machine Server  {
    var got: seq[int];
    start state Init  {
        on Hello do (x: int)  {
            got += (sizeof(got), x);
            if (sizeof(got) == 5)  {
                if (got[0] == 2 && got[1] == 0 && got[2] == 4 && got[3] == 1 && got[4] == 3)  {
                    assert false, "order-0";
                }
                if (got[0] == 1 && got[1] == 4 && got[2] == 0 && got[3] == 3 && got[4] == 2)  {
                    assert false, "order-1";
                }
                if (got[0] == 3 && got[1] == 2 && got[2] == 4 && got[3] == 0 && got[4] == 1)  {
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
        }
    }
}
test tcMain [main=Main]:  {
    Main, Server, Client
}
;
