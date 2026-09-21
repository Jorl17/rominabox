// Exercise the platform viewport-restore branch from the actual GL2 source.
// Cocoa can invalidate GL state without a size change. Simulate that case
// without opening a window or running an emulator.
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
const root=path.resolve(import.meta.dirname,'../..');
const sourcePath=process.argv[2] || path.join(root,'vendor/retroarch/gfx/drivers/gl2.c');
const source=fs.readFileSync(sourcePath,'utf8');
// The renderchain function appears before gl2_frame in this pinned source.
const marker=source.indexOf('/* Apparently the viewport is lost each frame');
const directive=source.lastIndexOf('#if',marker);
const end=source.indexOf('#endif',marker)+6;
if(marker<0 || directive<0)throw Error('Viewport restore branch missing');
const branch=source.slice(directive,end);
const out=path.join(root,'work/cocoa-viewport-check');fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(out+'/check.c',`#include <assert.h>\n#define OSX 1\nstatic unsigned applied;\nstatic void gl2_set_viewport(void *g,unsigned w,unsigned h,int full,int rotate){assert(!full && rotate);++applied;}\n#define false 0\n#define true 1\nstatic void frame(void){void *gl=0;unsigned width=1000,height=600;
${branch}
}\nint main(void){frame();frame();return applied==2?0:1;}\n`);
execFileSync('cc',[out+'/check.c','-o',out+'/check']);execFileSync(out+'/check',[],{stdio:'inherit'});
console.log('Cocoa restores core viewport on each frame, independent of resize notifications');
