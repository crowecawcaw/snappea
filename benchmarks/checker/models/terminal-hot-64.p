event A;
event B;
event C;
spec Terminal observes A,B,C  {
    start state Cold  {
        on A goto HotA;
        on B goto HotB;
        on C goto HotC;
    }
    hot state HotA  {
    }
    hot state HotB  {
    }
    hot state HotC  {
    }
}
machine Main  {
    start state Init  {
        entry  {
            var n:int;
            n=choose(64);
            if (n==0)  {
                announce A;
            }
            else if (n==1)  {
                announce B;
            }
            else if (n==2)  {
                announce C;
            }
        }
    }
}
test tcMain [main=Main]: assert Terminal in  {
    Main
}
;
