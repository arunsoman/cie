// Read-only impact from the pinned storage revision; never invokes ingestion.
import { NextRequest, NextResponse } from 'next/server';
import { validateLocalPath } from '@/lib/analyze-jobs';
import { storageCall, withRevision, StorageFailure } from '@/lib/storage-native';
export const runtime = 'nodejs';
export async function POST(req: NextRequest) {
  let body: { sourceRoot?: string; symbol?: string; revision?: string };
  try { body = await req.json(); } catch { return NextResponse.json({error:'Invalid request'}, {status:400}); }
  const root = validateLocalPath(body.sourceRoot ?? '');
  const name = String(body.symbol ?? '').trim();
  if (!root.ok || !name || name.length > 300) return NextResponse.json({error:'Select an ingested local repository and a symbol.'},{status:400});
  try {
    return await withRevision(root.path, async context => {
      const response = await storageCall<{ outputs: {data: {items: {id: string}[]}}[] }>({op:'retrieve',context,request:{query:{select:{kind:'symbol',filter:{or:[{field:{field:'qualified_name',op:'eq',value:name}},{field:{field:'name',op:'eq',value:name}}]}}},outputs:[{id:'matches',output:{entities:{page:{limit:100}}}}]}});
      const candidates = response.outputs[0].data.items;
      if (candidates.length !== 1) return NextResponse.json({error:candidates.length ? 'That symbol name is ambiguous; use its qualified name.' : 'Symbol not found.',candidates},{status:candidates.length ? 409 : 404});
      const impact = await storageCall<{outputs: {data: unknown}[]}>({op:'retrieve',context,request:{query:{select:{kind:'symbol',filter:{ids:[candidates[0].id]}}},outputs:[{id:'impact',output:{detail:'impact'}}]}});
      return NextResponse.json(impact.outputs[0].data);
    }, body.revision);
  } catch(error) { return NextResponse.json({error: error instanceof Error ? error.message : 'Storage retrieval failed',code:error instanceof StorageFailure ? error.code : 'Io'}, {status:503}); }
}
