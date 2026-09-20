import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
const source = await readFile(new URL('../src/dom/rate_limit.js', import.meta.url), 'utf8');
const text = 'Too many requests\nYou’re making requests too quickly. We’ve temporarily limited access to your conversations to protect your data.\nPlease wait a few minutes before trying again.\nGot it';
function observe({body=text, visible=true, message=false, dialogs=1}={}) {
  class Element {
    getClientRects(){return visible ? [{}] : [];}
    checkVisibility(){return visible;}
    closest(){return message ? {} : null;}
  }
  const dialog=Object.assign(new Element(),{innerText:body});
  const document={querySelectorAll:selector=>{
    assert.equal(selector,'[role="dialog"], [role="alertdialog"]');
    return Array(dialogs).fill(dialog);
  }};
  return vm.runInNewContext(`(${source})`,{HTMLElement:Element,document})();
}
test('observed service restriction is recognized without account or message content',()=>{
  assert.equal(observe(),true);
  assert.equal(observe({body:text.replace(/[’]/g,"'").replace(/\n/g,'  ')}),true);
  for (const options of [{visible:false},{dialogs:0},{message:true},{body:'Too many requests'},
    {body:'Conversation: '+text},{body:text.replace('Please wait a few minutes before trying again.','Try now')}]) {
    assert.equal(observe(options),false);
  }
});
