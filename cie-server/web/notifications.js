"use strict";
// Deletion clears cached source/derived material in every open tab of this
// workspace origin. The message carries no repository IDs, paths or source.
(()=>{
 const clearAndReload=()=>{document.querySelectorAll('input,textarea').forEach(field=>{field.value='';});location.reload();};
 let channel;
 try{channel=new BroadcastChannel('cie-evidence-deletion');channel.onmessage=event=>{if(event.data==='deleted')clearAndReload();};}catch{channel=null;}
 window.addEventListener('storage',event=>{if(event.key==='cie-evidence-deletion')clearAndReload();});
 window.cieNotifyDeletion=()=>{let sent=false;try{if(channel){channel.postMessage('deleted');sent=true;}}catch{}if(!sent){try{localStorage.setItem('cie-evidence-deletion',`${Date.now()}-${Math.random()}`);}catch{}}setTimeout(clearAndReload,50);};
})();
