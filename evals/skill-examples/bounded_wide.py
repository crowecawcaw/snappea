#!/usr/bin/env python3
"""Run the unchanged wide-bundle assertions with an explicit execution deadline."""
import argparse
import asyncio
import json
from pathlib import Path
import sys
sys.path.insert(0,'/grader/private')
from stress_wide import check

async def main(args):
    try:
        result=await asyncio.wait_for(check(args.solution),timeout=args.timeout)
        result['execution_status']='completed'
    except asyncio.TimeoutError:
        result={'solution':args.solution,'supplemental':True,'passed':False,
                'execution_status':'timeout','error':'probe did not finish within execution deadline'}
    result['timeout_seconds']=args.timeout
    Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('solution');p.add_argument('--output',required=True);p.add_argument('--timeout',type=float,default=10);a=p.parse_args();asyncio.run(main(a))
