import{t as e}from"./react-MZV-sySy.js";import{i as t,n,t as r}from"./pTokens-iJb68ucT.js";import{t as i}from"./jsx-runtime-DPk14AgX.js";var a=e(),o=i(),s={keyword:`k`,typeName:`ty`,className:`ev`,"variableName.special":`nm`,string:`s`,number:`n`,atom:`n`,comment:`c`,operator:`op`},c=class{string;pos=0;start=0;constructor(e){this.string=e}eol(){return this.pos>=this.string.length}next(){return this.pos<this.string.length?this.string[this.pos++]:void 0}skipToEnd(){this.pos=this.string.length}skipTo(e){let t=this.string.indexOf(e,this.pos);return t<0?!1:(this.pos=t,!0)}eatSpace(){let e=this.pos;for(;this.pos<this.string.length&&/\s/.test(this.string[this.pos]);)this.pos++;return this.pos>e}match(e){let t=this.string.slice(this.pos);if(typeof e==`string`)return t.startsWith(e)?(this.pos+=e.length,!0):!1;let n=t.match(e);return!n||n.index!==0?null:(this.pos+=n[0].length,n)}current(){return this.string.slice(this.start,this.pos)}};function l(e){let t=[],n=r.startState(2);return e.split(`
`).forEach((e,i)=>{i>0&&t.push(`
`);let a=new c(e);for(;!a.eol();){a.start=a.pos;let e=r.token(a,n),i=a.current(),c=e?s[e]:void 0;t.push(c?(0,o.jsx)(`span`,{className:c,children:i},t.length):i)}}),t}var u={python:f(`def with as import from assert return class if else for in pass raise try except lambda None True False`),rust:f(`fn let mut use pub struct impl enum match if else for in return mod move static const true false Some None Ok Err self Self as ref where`),go:f(`func package import var const type struct return if else for range defer go nil true false map chan interface`),java:f(`import package class public private static final void new return try catch var if else throw throws record true false null`),kotlin:f(`val var fun if else in setOf dependencies`)},d=new Set([`python`,`toml`,`yaml`,`shell`]);function f(e){return new Set(e.split(` `))}function p(e,t){let n=d.has(t)?`#.*$`:t===`xml`?`<!--[\\s\\S]*?-->`:`//.*$`,r=RegExp(`(${n})|("(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)*')|(@[A-Za-z]\\w*)|\\b([A-Za-z_]\\w*)\\b`,`gm`),i=u[t],a=[],s=0;for(let t of e.matchAll(r)){let n=t.index??0;n>s&&a.push(e.slice(s,n));let r=t[1]?`c`:t[2]?/^["']e[A-Z]/.test(t[2])?`ev`:`s`:t[3]?`nm`:i?.has(t[4])?`k`:void 0;a.push(r?(0,o.jsx)(`span`,{className:r,children:t[0]},n):t[0]),s=n+t[0].length}return a.push(e.slice(s)),a}function m({lang:e,children:t,title:n}){let r=t.replace(/^\n/,``).replace(/\s+$/,``),i=(0,a.useMemo)(()=>e===`p`?l(r):e===`shell`||e===`text`?[r]:p(r,e),[e,r]);return(0,o.jsxs)(`figure`,{className:`code lang-${e}`,children:[n&&(0,o.jsx)(`figcaption`,{children:n}),(0,o.jsx)(`pre`,{children:(0,o.jsx)(`code`,{children:i.map((e,t)=>(0,o.jsx)(a.Fragment,{children:e},t))})})]})}var h=`machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`,g=`machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`,_=`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`;function v(e,t){let n=e.split(`
`);return t.map(e=>{let t=n.find(t=>t.includes(e));if(t===void 0)throw Error(`miniModel: no line contains ${e}`);return t.trim()}).join(`
`)}var y=v(h,[`send input.counter, eRead`,`receive {`,`send input.counter, eWrite`]),b=v(g,[`send input.counter, eIncrement`]),x=v(g,[`on eIncrement do`]);function S(){let[e,r]=(0,a.useState)(h),[i,s]=(0,a.useState)(_),[c,l]=(0,a.useState)(`model`),[u,d]=(0,a.useState)(!1),[f,p]=(0,a.useState)(!1),[v,S]=(0,a.useState)(!1),[C,w]=(0,a.useState)(),[T,E]=(0,a.useState)(),D=(0,a.useRef)(void 0),O=(0,a.useRef)(void 0),k=(0,a.useRef)(null),A=c===`model`?e:i,j=i===`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`?e===`machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`?`broken`:e===`machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`?`fixed`:void 0:void 0,M=f||!j;(0,a.useEffect)(()=>()=>{D.current?.terminate(),clearTimeout(O.current)},[]),(0,a.useEffect)(()=>{u&&k.current?.focus()},[u,c]);function N(){D.current?.terminate(),D.current=void 0,clearTimeout(O.current),S(!1),w(void 0),E(void 0)}function P(e){N(),r(e),s(_),l(`model`),d(!1),p(!1)}function F(){N(),S(!0);try{let t=new Worker(new URL(new URL(`p.worker-CAStzHRP.js`,import.meta.url).href,``+import.meta.url),{type:`module`});D.current=t;function n(){t.terminate(),D.current===t&&(D.current=void 0),clearTimeout(O.current),S(!1)}t.onmessage=({data:e})=>{D.current!==t||e.progress||(n(),e.error?E(e.error):e.result?.kind===`check`?w(e.result):E(`The checker returned an unexpected response. Try running again.`))},t.onerror=()=>{D.current===t&&(n(),E(`The browser could not start the checker. Try again or open the full Playground.`))},O.current=setTimeout(()=>{n(),E(`The checker did not respond. Try again or open the full Playground.`)},3e4);let r={kind:`check`,files:{"Model.p":e,"Test.p":i},testCase:`tcTwoClients`,timeout:2,seed:42,strict:!1,extraArgs:[`--no-mutation`]};t.postMessage({id:1,req:r})}catch{D.current?.terminate(),D.current=void 0,clearTimeout(O.current),S(!1),E(`The browser could not start the checker. Try the full Playground.`)}}let I=t({files:{"Model.p":e,"Test.p":i},open:`Model.p`,testCase:`tcTwoClients`,timeout:2,extraArgs:`--no-mutation`}),L=I.length<=n,R=L?`./playground.html#s=${I}`:`./playground.html`,z=C?.exitCode===0?`No bug found`:C?.bug?`Counterexample found`:`Check needs attention`;return(0,o.jsxs)(`div`,{className:`mini-playground`,children:[(0,o.jsxs)(`div`,{className:`mini-toolbar`,children:[(0,o.jsxs)(`div`,{className:`mini-presets`,role:`group`,"aria-label":`Counter design`,children:[(0,o.jsx)(`button`,{"aria-pressed":e===`machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`&&i===`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`,onClick:()=>P(h),children:`Read then write`}),(0,o.jsx)(`button`,{"aria-pressed":e===`machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`&&i===`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`,onClick:()=>P(g),children:`Atomic increment`})]}),(0,o.jsxs)(`div`,{className:`mini-run-controls`,children:[C&&(0,o.jsx)(`span`,{className:C.exitCode===0?`mini-pass`:`mini-error`,role:`status`,children:z}),T&&(0,o.jsx)(`span`,{className:`mini-error`,role:`status`,children:`Check could not run`}),(0,o.jsx)(`button`,{className:`button primary`,onClick:F,disabled:v,children:v?`Checking…`:`Run model`}),v&&(0,o.jsx)(`button`,{onClick:N,children:`Stop`})]})]}),(0,o.jsxs)(`div`,{className:`mini-workspace`,children:[(0,o.jsx)(`div`,{className:`mini-source`,children:M?(0,o.jsxs)(o.Fragment,{children:[(0,o.jsxs)(`div`,{className:`mini-filebar`,children:[(0,o.jsxs)(`div`,{role:`group`,"aria-label":`P source files`,children:[(0,o.jsx)(`button`,{"aria-pressed":c===`model`,onClick:()=>l(`model`),children:`Model.p`}),(0,o.jsx)(`button`,{"aria-pressed":c===`setup`,onClick:()=>l(`setup`),children:`Test.p`})]}),(0,o.jsxs)(`div`,{children:[j&&!u&&(0,o.jsx)(`button`,{onClick:()=>p(!1),children:`Show less`}),(0,o.jsx)(`button`,{onClick:()=>d(!u),children:u?`Done editing`:`Edit P code`})]})]}),u?(0,o.jsx)(`textarea`,{ref:k,className:`mini-editor`,"aria-label":`Edit ${c===`model`?`Model.p`:`Test.p`}`,spellCheck:!1,value:A,onChange:e=>{N(),c===`model`?r(e.target.value):s(e.target.value)}}):(0,o.jsx)(`div`,{className:`mini-code`,role:`region`,"aria-label":`Read ${c===`model`?`Model.p`:`Test.p`}`,tabIndex:0,children:(0,o.jsx)(m,{lang:`p`,children:A})})]}):(0,o.jsxs)(`div`,{className:`mini-focus`,children:[(0,o.jsxs)(`p`,{className:`mini-label`,children:[j===`broken`?`Each client reads, then writes`:`Each client sends one message`,` `,(0,o.jsx)(`span`,{children:`Model.p`})]}),(0,o.jsx)(m,{lang:`p`,children:j===`broken`?y:b}),j===`fixed`&&(0,o.jsxs)(o.Fragment,{children:[(0,o.jsxs)(`p`,{className:`mini-label`,children:[`The counter adds one in a single step `,(0,o.jsx)(`span`,{children:`Model.p`})]}),(0,o.jsx)(m,{lang:`p`,children:x})]}),(0,o.jsxs)(`p`,{className:`mini-label`,children:[`The test starts two clients, then checks the total `,(0,o.jsx)(`span`,{children:`Test.p`})]}),(0,o.jsx)(m,{lang:`p`,children:`assert final == 2;`}),(0,o.jsx)(`button`,{onClick:()=>p(!0),children:`Show the full model`})]})}),(0,o.jsxs)(`div`,{className:`mini-result`,children:[i!==`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`&&(0,o.jsx)(`p`,{className:`mini-edited-note`,children:`Test.p has been edited. The checker will use your edited assertions.`}),(0,o.jsx)(`div`,{className:`mini-status`,role:`status`,"aria-live":`polite`,children:T?(0,o.jsx)(`p`,{className:`mini-error`,children:T}):v?(0,o.jsx)(`p`,{children:`Loading the checker and trying message orderings…`}):C?(0,o.jsxs)(o.Fragment,{children:[(0,o.jsx)(`h3`,{className:C.exitCode===0?`mini-pass`:`mini-error`,children:z}),(0,o.jsx)(`p`,{children:C.exitCode===0?`This checks the model within its bounds, not your implementation. Read the output below for warnings.`:C.bug?`One ordering broke a check. The output and trace are below.`:`Read the diagnostics below, edit the code, and run again.`})]}):j===`broken`?(0,o.jsxs)(o.Fragment,{children:[(0,o.jsx)(`h3`,{children:`The checker finds an ordering that loses an increment`}),(0,o.jsxs)(`ol`,{className:`mini-steps`,children:[(0,o.jsxs)(`li`,{children:[`Client A reads `,(0,o.jsx)(`b`,{children:`0`})]}),(0,o.jsxs)(`li`,{children:[`Client B reads `,(0,o.jsx)(`b`,{children:`0`})]}),(0,o.jsxs)(`li`,{children:[`Client A writes `,(0,o.jsx)(`b`,{children:`1`})]}),(0,o.jsxs)(`li`,{children:[`Client B writes `,(0,o.jsx)(`b`,{children:`1`})]})]}),(0,o.jsx)(`p`,{className:`mini-fail`,children:`Assertion failed: expected 2 increments, counter is 1`}),(0,o.jsxs)(`p`,{className:`mini-note`,children:[`This is the result for the model as written. Run it to get the checker's own output, then try `,(0,o.jsx)(`b`,{children:`Atomic increment`}),`.`]})]}):j===`fixed`?(0,o.jsxs)(o.Fragment,{children:[(0,o.jsx)(`h3`,{children:`The checker tries every ordering and none fails`}),(0,o.jsx)(`p`,{children:`The counter handles one message at a time, so nothing can arrive between reading the value and writing it.`}),(0,o.jsx)(`p`,{className:`mini-note`,children:`This is the result for the model as written. Run it to get the checker's own output.`})]}):(0,o.jsx)(`p`,{children:`Run the model to check your edit.`})})]})]}),C&&(0,o.jsxs)(`div`,{className:`mini-output`,children:[(0,o.jsxs)(`details`,{open:!0,children:[(0,o.jsx)(`summary`,{children:`Checker output`}),(0,o.jsx)(`pre`,{children:C.console})]}),C.traceText&&(0,o.jsxs)(`details`,{children:[(0,o.jsx)(`summary`,{children:`Counterexample trace`}),(0,o.jsx)(`pre`,{children:C.traceText})]})]}),(0,o.jsxs)(`div`,{className:`mini-footer`,children:[(0,o.jsx)(`span`,{children:`Runs in your browser; no code leaves this page. 2-second budget, mutation testing off.`}),(0,o.jsx)(`a`,{href:R,children:L?`Open these files in the full Playground →`:`Project too large for a link — open Playground and copy your files →`})]}),(0,o.jsx)(`noscript`,{children:(0,o.jsxs)(`p`,{children:[`Enable JavaScript to edit and run this model, or follow the `,(0,o.jsx)(`a`,{href:`./docs/first-model.html`,children:`tutorial`}),`.`]})})]})}export{S as MiniPlayground};