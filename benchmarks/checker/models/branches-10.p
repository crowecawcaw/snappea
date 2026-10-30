machine Main  {
    start state Init  {
        entry  {
            var ok: bool;
            var n: int;
            var target: int;
            target = choose(3);
            ok = true;
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok)  {
                if ($)  {
                    n = n + 1;
                }
                else  {
                    ok = false;
                }
            }
            if (ok && target == 0)  {
                assert false, "branch-0";
            }
            if (ok && target == 1)  {
                assert false, "branch-1";
            }
            if (ok && target == 2)  {
                assert false, "branch-2";
            }
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
