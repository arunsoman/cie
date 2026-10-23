// Captured source bytes from the same revision as the displayed symbol.
import { NextRequest, NextResponse } from 'next/server';
import { validateLocalPath } from '@/lib/analyze-jobs';
import { storageCall, withRevision } from '@/lib/storage-native';
export const runtime = 'nodejs';
export async function POST(req: NextRequest) {
  let body: {root?:string; path?:string; startByte?:number; endByte?:number; revision?:string};
  try {body=await req.json();}catch{return NextResponse.json({error:'Invalid request'},{status:400});}
  const root=validateLocalPath(body.root ?? '');
  const start=body.startByte, end=body.endByte;
  if (!root.ok || !body.path || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start! < 0 || end! < start!) return NextResponse.json({error:'Invalid source selection'},{status:400});
  if (end! - start! > 256*1024) return NextResponse.json({error:'Select a smaller source range'},{status:413});
  try {
    return await withRevision(root.path,async context=>{
      const response=await storageCall<{outputs:{data:{text:string;start:number;end:number;content_hash:string}}[]}>({op:'retrieve',context,request:{query:{select:{kind:'file',filter:{field:{field:'path',op:'eq',value:body.path}}}},outputs:[{id:'source',output:{detail:{source:{start,end}}}}]}});
      const source=response.outputs[0].data;
      return NextResponse.json({path:body.path,startByte:source.start,endByte:source.end,text:source.text,truncated:false,contentHash:source.content_hash,revision:body.revision,source:'captured'});
    },body.revision);
  }catch(error){return NextResponse.json({error:error instanceof Error ? error.message : 'Captured source unavailable'},{status:404});}
}
