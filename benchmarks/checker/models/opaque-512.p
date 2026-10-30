machine Main  {
    start state Init  {
        entry  {
            var n: int;
            n = choose(512);
            assert n != 17, "opaque-0";
            assert n != 233, "opaque-1";
            assert n != 487, "opaque-2";
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
