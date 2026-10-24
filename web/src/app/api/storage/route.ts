// Application transport for the same in-process storage library.
import { NextRequest, NextResponse } from 'next/server';
import { validateLocalPath } from '@/lib/analyze-jobs';
import { storageCall, StorageFailure } from '@/lib/storage-native';
export const runtime='nodejs';
export async function POST(req:NextRequest){
  let body:Record<string,unknown>;
  try {const text=await req.text();if(text.length>1024*1024)return NextResponse.json({error:'Request too large'},{status:413});body=JSON.parse(text);}catch{return NextResponse.json({error:'Invalid JSON'},{status:400});}
  try {
    if(body.context==null && body.request && typeof body.request==='object') body.context=(body.request as Record<string,unknown>).context;
    if(body.op==='open' || body.op==='revisions' || (body.op==='describe' && body.context==null)){
      const root=validateLocalPath(typeof body.root==='string'?body.root:'');
      if(!root.ok)return NextResponse.json({error:root.plain},{status:400});
      return NextResponse.json(await storageCall({op:body.op,root:root.path,revision:body.revision,limit:body.limit,after:body.after,request:body.request}));
    }
    if(!['describe','retrieve','tool_retrieve','resolve','capabilities','explain','close'].includes(String(body.op)) || typeof body.context!=='string')return NextResponse.json({error:'Unknown operation or missing context'},{status:400});
    return NextResponse.json(await storageCall({op:body.op,context:body.context,request:body.request,lookup:body.lookup},req.signal));
  }catch(error){return NextResponse.json({error:error instanceof Error?error.message:'Storage request failed',code:error instanceof StorageFailure?error.code:'Io'},{status:error instanceof StorageFailure&&error.code==='CursorExpired'?410:400});}
}
