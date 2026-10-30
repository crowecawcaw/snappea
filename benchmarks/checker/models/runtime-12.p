event Unexpected;
event Never;
machine Main  {
    start state Init  {
        entry  {
            var mode: int;
            var ok: bool;
            mode = choose(3);
            ok = true;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            ok = ok && $;
            if (ok)  {
                if (mode == 0)  {
                    assert false, "runtime-assert";
                }
                else if (mode == 1)  {
                    send this, Unexpected;
                }
                else  {
                    receive  {
                        case Never:  {
                        }
                    }
                }
            }
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
