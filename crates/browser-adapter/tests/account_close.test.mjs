import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
const source = await readFile(new URL('../src/dom/close_account_settings.js', import.meta.url), 'utf8');
const closedSource = await readFile(new URL('../src/dom/settings_closed.js', import.meta.url), 'utf8');
function observe({dialogs=1, settings=true, buttons=1, label='Close', disabled=false, blocked=false}={}, script=source) {
  class Element {
    getClientRects(){return [{}];}
    checkVisibility(){return true;}
    closest(){return null;}
    getBoundingClientRect(){return {left:0,top:0,width:20,height:20};}
    contains(node){return node===this;}
  }
  const button=Object.assign(new Element(), {disabled,textContent:'X',getAttribute:()=>label});
  const dialog=Object.assign(new Element(),{querySelectorAll:selector=>selector==='[role="tab"]'
    ? (settings?['General','Account']:['Other']).map(textContent=>Object.assign(new Element(),{textContent}))
    : Array(buttons).fill(button)});
  const document={querySelectorAll:()=>Array(dialogs).fill(dialog),elementFromPoint:()=>blocked?null:button};
  return vm.runInNewContext(`(${script})`,{HTMLElement:Element,document})();
}
test('only one accessible settings close control supplies a pointer target',()=>{
  assert.equal(observe().x,10);
  assert.equal(observe({label:'Close settings'}).y,10);
  for(const options of [{dialogs:0},{dialogs:2},{settings:false},{buttons:0},{buttons:2},{label:'Log out'},{disabled:true},{blocked:true}])assert.equal(observe(options).x,undefined);
  assert.equal(JSON.stringify(observe({label:'private-user-data'})).includes('private-user-data'),false);
});
test('settings cleanup tracks only its owned settings surface',()=>{
  assert.equal(observe({},closedSource),false);
  assert.equal(observe({dialogs:2},closedSource),false);
  assert.equal(observe({dialogs:0},closedSource),true);
  assert.equal(observe({settings:false},closedSource),true);
});
