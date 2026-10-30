machine Main  {
    start state Init  {
        entry  {
            var n: int;
            n = choose(2048);
            assert n != 43, "opaque-0";
            assert n != 971, "opaque-1";
            assert n != 1807, "opaque-2";
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
