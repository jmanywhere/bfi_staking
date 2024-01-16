import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BfiStaking } from "../target/types/bfi_staking";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { MintLayout, createMint, getMint, getMintCloseAuthority, getOrCreateAssociatedTokenAccount, mintTo, setAuthority, transfer } from "@solana/spl-token";
import { assert } from "chai";
import { waitForTransaction } from "./helpers";
describe("bfi_staking", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const payer = provider.wallet as anchor.Wallet;
  const connection = new Connection(
    // "http://127.0.0.1:8899"
    clusterApiUrl("mainnet-beta")
    , "confirmed");
  const mintKeyPair = new PublicKey("u6EzDxvJEzMGDqmiXGng757TcXjQ8fxKQN8uHRRCrqC");
  // Keypair.fromSecretKey(new Uint8Array([
  //   175,  37, 203, 248,  22, 218,  17, 178, 109, 137,
  //   172, 167,  29, 163, 246,   1,  93,  84, 247, 226,
  //   228, 211, 250,  70, 248, 234,  62, 161, 135, 238,
  //   193, 239,  31, 229,  49, 154, 127, 205, 104, 109,
  //    45, 229, 169, 246, 135, 106,  26, 138, 160, 151,
  //   115, 218, 179, 108,  62, 231,  10, 165,  76, 188,
  //    85, 188, 144, 213
  // ]));
  // console.log(mintKeyPair)

  const program = anchor.workspace.BfiStaking as Program<BfiStaking>;
  const newOwner = new PublicKey("EmRTfCiBAiN8LPBZu5zdkiMmuUnU18GddBcwuEHUULmb")

  // async function createMintToken(){
  //   const mintLamportBalance = await connection.getBalance(mintKeyPair.publicKey, "confirmed")
  //   if( mintLamportBalance > 0 ){
  //     console.log("Mint already created")
  //     return;
  //   }
  //   const mint = await createMint(connection, payer.payer, payer.publicKey, payer.publicKey, 9, mintKeyPair)
  //   console.log(mint)

  //   const userTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     connection,
  //     payer.payer,
  //     mintKeyPair.publicKey,
  //     payer.publicKey
  //   )

  //   await mintTo(
  //     connection,
  //     payer.payer,
  //     mintKeyPair.publicKey,
  //     userTokenAccount.address,
  //     payer.publicKey,
  //     1000 * 1e9
  //   )
  // }

  it("Is initialized!", async () => {
    // await createMintToken();
    let [vaultAccount] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
    let [statusAccount] = PublicKey.findProgramAddressSync([Buffer.from("status")], program.programId);
    console.log({ vault: vaultAccount.toBase58(), status: statusAccount.toBase58()})

    try{
      // Add your test here.
      const tx = await program.methods.initialize()
      .accounts({
        signer: payer.publicKey,
        tokenVault: vaultAccount,
        mint: mintKeyPair,
        status: statusAccount,
      })
      .rpc();
  
      console.log("Init TX:", tx);
  
      await waitForTransaction(connection, tx);

    }
    catch(e){
      console.log('TX ERR',e)
    }
    const statusInfo = await program.account.status.fetch(statusAccount);

    assert.equal(statusInfo.token.toBase58(), mintKeyPair.toBase58() )

    const mintObj = await getMint(connection, mintKeyPair)
    if( mintObj.mintAuthority !== vaultAccount){
      await setAuthority(
        connection,
        payer.payer,
        mintKeyPair,
        payer.publicKey,
        0,
        vaultAccount
      )
      console.log('transfer authority to vault')

      const newMint = await getMint(connection, mintKeyPair)
      assert.equal(newMint.mintAuthority.toBase58(), vaultAccount.toBase58())
      console.log('authority transfered')

      await setAuthority(
        connection,
        payer.payer,
        mintKeyPair,
        payer.publicKey,
        1,
        newOwner
      )
    }
  });

  it("Created Pool", async () => {

    let [newPoolAccount1] = PublicKey.findProgramAddressSync([Buffer.from("pool"), new Uint8Array([1]) ], program.programId);
    let [newPoolAccount2] = PublicKey.findProgramAddressSync([Buffer.from("pool"), new Uint8Array([2]) ], program.programId);
    let [newPoolAccount3] = PublicKey.findProgramAddressSync([Buffer.from("pool"), new Uint8Array([3]) ], program.programId);
    let [statusAccount] = PublicKey.findProgramAddressSync([Buffer.from("status")], program.programId);
    const time1 = 60 * 24 * 3600;
    const time2 = 180 * 24 * 3600;
    const time3 = 360 * 24 * 3600;
    try{
      const tx = await program.methods.createPool(1,60, new anchor.BN(time1.toString()))
        .accounts({
          signer: payer.publicKey,
          newPool: newPoolAccount1,
          status: statusAccount
        })
        .rpc();
      const tx1 = await program.methods.createPool(2,220, new anchor.BN(time2.toString()))
        .accounts({
          signer: payer.publicKey,
          newPool: newPoolAccount2,
          status: statusAccount
        })
        .rpc();
      const tx2 = await program.methods.createPool(3,600, new anchor.BN(time3.toString()))
        .accounts({
          signer: payer.publicKey,
          newPool: newPoolAccount3,
          status: statusAccount
        })
        .rpc();
      
      console.log("Create Pool 1 TX:", tx);
      await waitForTransaction(connection, tx);
      console.log("Create Pool 2 TX:", tx1);
      await waitForTransaction(connection, tx1);
      console.log("Create Pool 3 TX:", tx2);
      await waitForTransaction(connection, tx2);

    }
    catch(e){
      console.log(e)
    }
    const statusInfo = await program.account.status.fetch(statusAccount);
    assert.equal(statusInfo.totalPools, 1);
    const poolInfo = await program.account.poolInfo.fetch(newPoolAccount);
    assert.equal(poolInfo.basisPoints, 60)
    assert.equal(poolInfo.lockTime.toNumber(), time1)
  })

  it("Transfer Ownership And Tokens", async() => {
    let [statusAccount] = PublicKey.findProgramAddressSync([Buffer.from("status")], program.programId);

    try {

      const tx = await program.methods.transferOwnership().accounts({
        signer: payer.publicKey,
        status: statusAccount,
        newOwner: newOwner
      }).rpc();
      console.log("Change Owner:", tx);
      await waitForTransaction(connection, tx);


    }
    catch(e){
      console.log("Something failed... jeeze",e)
    }

    const statusInfo = await program.account.status.fetch(statusAccount);
    assert.equal(statusInfo.owner.toBase58(), newOwner.toBase58())
  })

  // it("Should stake", async () => {
  //   const userTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     connection,
  //     payer.payer,
  //     mintKeyPair.publicKey,
  //     payer.publicKey
  //   )

  //   const [poolInfoAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("pool"), new Uint8Array([1])],
  //     program.programId
  //   )

  //   const [stakeStatusAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("status")],
  //     program.programId,
  //   )

  //   const [userStakingPositionAccount] = PublicKey.findProgramAddressSync(
  //     [payer.publicKey.toBuffer(), new Uint8Array([1])],
  //     program.programId
  //   )

  //   let [vaultAccount] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
  
  //   try{

  //     const tx = await program.methods
  //       .stake(1, new anchor.BN("1000000000"))
  //       .accounts({
  //         signer: payer.publicKey,
  //         pool: poolInfoAccount,
  //         status: stakeStatusAccount,
  //         userTokenAccount: userTokenAccount.address,
  //         stakingPosition: userStakingPositionAccount,
  //         tokenVault: vaultAccount,
  //         mint: mintKeyPair.publicKey,
  //       })
  //       .rpc();
  //       console.log("Stake TX:", tx);
  //       await waitForTransaction(connection, tx);
  //   }
  //   catch(e){
  //     console.log(e)
  //   }
  //   const vaultBalance = await connection.getTokenAccountBalance(vaultAccount);
  //   assert.equal(vaultBalance.value.amount, "1000000000", "Incorrect Vault Balance");

  //   const userStakingPosition = await program.account.stakingPosition.fetch(userStakingPositionAccount);

  //   assert.equal(userStakingPosition.amount.toNumber(), 1e9, "Incorrect Staked Amount")
  //   assert.notEqual(userStakingPosition.startTime.toNumber(),0, "Start time incorrectly set")

  //   const statusInfo = await program.account.status.fetch(stakeStatusAccount);
  //   assert.equal(statusInfo.totalStaked.toNumber(), 1e9, "Incorrect total staked")
  // })
  // it("Should withdraw all", async () => {
  //   const userTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     connection,
  //     payer.payer,
  //     mintKeyPair.publicKey,
  //     payer.publicKey
  //   )

  //   const [poolInfoAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("pool"), new Uint8Array([1])],
  //     program.programId
  //   )

  //   const [stakeStatusAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("status")],
  //     program.programId,
  //   )

  //   const [userStakingPositionAccount] = PublicKey.findProgramAddressSync(
  //     [payer.publicKey.toBuffer(), new Uint8Array([1])],
  //     program.programId
  //   )

  //   let [vaultAccount] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
  
  //   try{

  //     const tx = await program.methods
  //       .withdraw(1)
  //       .accounts({
  //         signer: payer.publicKey,
  //         pool: poolInfoAccount,
  //         status: stakeStatusAccount,
  //         userTokenAccount: userTokenAccount.address,
  //         stakingPosition: userStakingPositionAccount,
  //         tokenVault: vaultAccount,
  //         mint: mintKeyPair.publicKey,
  //       })
  //       .rpc();
  //       console.log("Withdraw TX:", tx);
  //       await waitForTransaction(connection, tx);
  //     }
  //   catch(e){
  //     console.log(e)
  //   }
  //   const vaultBalance = await connection.getTokenAccountBalance(vaultAccount);
  //   assert.equal(vaultBalance.value.uiAmount, 0.05, "Incorrect Vault Balance");
  // })
  // it("Should claim tokens", async () => {
  //   const userTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     connection,
  //     payer.payer,
  //     mintKeyPair.publicKey,
  //     payer.publicKey
  //   )

  //   const [poolInfoAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("pool"), new Uint8Array([1])],
  //     program.programId
  //   )

  //   const [stakeStatusAccount] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("status")],
  //     program.programId,
  //   )

  //   const [userStakingPositionAccount] = PublicKey.findProgramAddressSync(
  //     [payer.publicKey.toBuffer(), new Uint8Array([1])],
  //     program.programId
  //   )

  //   let [vaultAccount] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
  
  //   try{
  //     const tx = await program.methods.stake(1, new anchor.BN("1000000000"))
  //       .accounts({
  //         signer: payer.publicKey,
  //         pool: poolInfoAccount,
  //         status: stakeStatusAccount,
  //         userTokenAccount: userTokenAccount.address,
  //         stakingPosition: userStakingPositionAccount,
  //         tokenVault: vaultAccount,
  //         mint: mintKeyPair.publicKey,
  //       }).rpc();
  //       console.log("Stake TX:", tx);
  //       await waitForTransaction(connection, tx);
  //     }
  //   catch(e){
  //     console.log(e)
  //   }

  //   // delay 11 seconds
  //   await new Promise((resolve) => setTimeout(resolve, 11000));

  //   try{
  //     const tx = await program.methods.claim(1)
  //       .accounts({
  //         signer: payer.publicKey,
  //         pool: poolInfoAccount,
  //         status: stakeStatusAccount,
  //         userTokenAccount: userTokenAccount.address,
  //         stakingPosition: userStakingPositionAccount,
  //         tokenVault: vaultAccount,
  //         mint: mintKeyPair.publicKey,
  //       }).rpc();
  //       console.log("Claim TX:", tx);
  //       await waitForTransaction(connection, tx);
  //     }
  //   catch(e){
  //     console.log(e)
  //   }
  //   const userTokenBalance = await connection.getTokenAccountBalance(userTokenAccount.address);
  //   // actual user balance is 1001.55 = 1000.6 - 0.05 (from previous withdraw)
  //   assert.equal(userTokenBalance.value.uiAmount, 1000.55, "Incorrect User Token Balance");
  // })
});
