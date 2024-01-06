import { Connection } from "@solana/web3.js";

export async function waitForTransaction(connection: Connection, tx: string){
  const latestblock = await connection.getLatestBlockhash("confirmed");
  await connection.confirmTransaction({
    ...latestblock,
    signature: tx,
  })
}