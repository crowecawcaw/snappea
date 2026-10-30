machine Main  {
    start state Init  {
        entry  {
            var mode: int;
            var ok: bool;
            mode=choose(100);
            ok=true;
            if (mode < 90)  {
                assert false, "common";
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok && mode < 99)  {
                assert false, "rare-prefix";
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok)  {
                ok = $;
            }
            if (ok && mode == 99)  {
                assert false, "rarest-prefix";
            }
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
