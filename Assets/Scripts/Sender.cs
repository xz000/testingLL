using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System;
using System.Linq;
using Bond;
using Steamworks;

public class Sender : MonoBehaviour
{
    public TestMenu02 testMenu02;
    public HostToggle theHT;
    public ClientToggle theCT;
    public TestSteamworks tss;
    public Image SignalLight;
    //public byte[] buffer;
    public bool started = false;
    public static int clientNum;
    public SkillsLink SPNL;
    public NetWriter MyNS;
    //public HostTopology HTo;
    public static int HSID;
    public static int CNID;
    public int CHANID;
    byte[] rcbuffer = new byte[256];
    public int rcbfsz = 256;
    public Toggle CCToggle;
    public CanvasGroup MCG;
    public MainSkillMenu MSM;
    public static CSteamID roomid;
    public static CSteamID[] TOmb;
    //public static GameState NowState;
    bool Learning = false;
    bool WaitingSkillLevels;
    float TimeCount = 0;
    public static float LearnTime = 5;
    public int RoundNow = -10;
    public static int TotalRounds = 2;
    //EndData sts;
    EndData[] Src;
    public static int[][] theSLtemp;
    bool[] SLtb;//Used by endingcompare
    bool[] Cntb;//Used by Connection Status Check & SkillLevel Temp check
    public AudioClip SoundAtStart;

    private void Start()
    {
        ResetSelf();
        //PrepareTemp(2, (int)SkillCode.SelfExplodeScript);
    }

    public void meStart()
    {
        ResetSelf();
        gameObject.SetActive(false);
    }

    public void PrepareTemp(int PlayersCount, int SkillsCount)
    {
        NetWriter.rs = PlayersCount;
        SLtb = new bool[PlayersCount];
        Cntb = new bool[PlayersCount];
        Src = new EndData[PlayersCount];
        theSLtemp = new int[PlayersCount][];
        for (int i = 0; i < PlayersCount; i++)
            theSLtemp[i] = new int[SkillsCount];
    }

    public void SetTempAndCheck(int cN, int[] cSL)
    {
        if (!Learning)
            return;
        Debug.Log(cN + "set:" + cSL[0]);
        for (int i = 0; i < cSL.Length; i++)
            theSLtemp[cN][i] = cSL[i];
        Cntb[cN] = true;
        Debug.Log(cN + " SLtemp Status: " + Cntb[cN]);
        if (AllOK(Cntb))
        {
            SendHello(5);
            Cntb = new bool[Cntb.Length];
            Debug.Log("Local Skill Level Temp Set,Hello5 Sent");
        }
    }

    public void SetCntbAndCheck(int cN)
    {
        Cntb[cN] = true;
        Debug.Log(cN + " Connection Status: " + Cntb[cN]);
        if (AllOK(Cntb))
        {
            testMenu02.SetGreen();
            Cntb = new bool[Cntb.Length];
        }
    }

    public void SetSLTempAndCheck(int cN)
    {
        Cntb[cN] = true;
        Debug.Log(cN + " SkillLevel Temp Status: " + Cntb[cN]);
        if (AllOK(Cntb))
        {
            Debug.Log("Connect Do");
            started = true;
            Learning = false;
            SignalLight.color = Color.green;
            MyNS.meEnable();//开启netwriter
            //MyNS.enabled = true;
            CCToggle.isOn = true;//开启ClickCatcher
            Cntb = new bool[Cntb.Length];
            //HideMC();
        }
    }

    public void ResetSelf()
    {
        started = false;
        SignalLight.color = Color.white;
        //MyNS.isstarted = false;
        //MyNS.enabled = false;
        MyNS.meDisable();
        CCToggle.isOn = false;
    }

    public void SendEnd(EndData se)
    {
        if (!started || Learning)
            return;
        Learning = true;
        WaitingSkillLevels = true;
        Debug.Log("Round++,Now:" + RoundNow);
        //sts = se;
        Src[Sender.clientNum] = se;
        SLtb[Sender.clientNum] = true;
        Debug.Log("End Message " + Sender.clientNum + " Set");
        EndingCompare();
        Bond.IO.Safe.OutputBuffer ob2 = new Bond.IO.Safe.OutputBuffer(128);
        Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer> boc = new Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob2);
        Serialize.To(boc, se);
        byte[] sendBytes = new byte[ob2.Data.Array.Length + 1];
        sendBytes[0] = (byte)2;
        ob2.Data.Array.CopyTo(sendBytes, 1);
        foreach (CSteamID i in TOmb)
        {
            if (i != SteamUser.GetSteamID())
                SteamNetworking.SendP2PPacket(i, sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
        }
        Debug.Log("End Sent");
        TotalRounds = int.Parse(SteamMatchmaking.GetLobbyData(Sender.roomid, "Total_Rounds"));
        LearnTime = int.Parse(SteamMatchmaking.GetLobbyData(Sender.roomid, "Learn_Time"));
    }

    public void RealEnd()
    {
        GameObject[] pcs = GameObject.FindGameObjectsWithTag("Player");
        //if (pcs.Length > 1) return;
        Debug.Log("Real End");
        MyNS.meDisable();//关闭netwriter
        CCToggle.isOn = false;//关闭ClickCatcher
        Debug.Log("Round " + RoundNow);
        foreach (GameObject pc in pcs)
            Destroy(pc);
        GameObject[] safeground = GameObject.FindGameObjectsWithTag("Ground");
        foreach (GameObject ground in safeground)
            Destroy(ground);
    }

    public void EndBattle(int pos)
    {
        //NowState = GameState.Learning;
        Bond.IO.Safe.InputBuffer ib2 = new Bond.IO.Safe.InputBuffer(rcbuffer);
        Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer> cbr = new Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer>(ib2);
        Src[pos] = Deserialize<EndData>.From(cbr);
        SLtb[pos] = true;
        Debug.Log("End Message " + pos + "Received And Set");
        EndingCompare();
    }

    void BattlesFinish()
    {
        MSM.CloseMainSkillMenu();
        Learning = false;
        WaitingSkillLevels = false;
        TimeCount = 0;
        ResetSelf();
        Debug.Log("All Battle Finished" + RoundNow);
        GameObject safeground = GameObject.FindGameObjectWithTag("Ground");
        Destroy(safeground);
        GameObject[] pcs = GameObject.FindGameObjectsWithTag("Player");
        foreach (GameObject pc in pcs)
            Destroy(pc);
        RoundNow = -10;
        TOmb = null;
        ShowMC();
        testMenu02.LeaveLobby();
        uint msgSize;
        while (SteamNetworking.IsP2PPacketAvailable(out msgSize))
        {
            byte[] packet = new byte[msgSize];
            CSteamID steamIDRemote;
            uint bytesRead = 0;
            if (SteamNetworking.ReadP2PPacket(packet, msgSize, out bytesRead, out steamIDRemote))
                Debug.Log("PACKET after Exit");
        }
    }

    public void ClearSArray()
    {
        SLtb = new bool[SLtb.Length];
        Src = new EndData[Src.Length];
        //sts = null;
    }

    bool AllOK(bool[] ba)
    {
        bool allok = true;
        for (int i = 0; i < ba.Length; i++)
        {
            allok = allok & ba[i];
            if (!allok) break;
        }
        return allok;
    }

    void EndingCompare()
    {
        if (!AllOK(SLtb))
            return;
        if (Src[0].CircleID == 666)
        {
            RoundNow = 1;
            ClearSArray();
            //MyNS.enabled = false;
            MyNS.meDisable();//关闭netwriter
            CCToggle.isOn = false;//关闭ClickCatcher
            HideMC();
            MSM.OpenMainSkillMenu();
            Debug.Log("received start message:\nRound " + RoundNow);
            return;
        }
        Debug.Log("Compareing Ending Place");
        if ((FixMath.Fix64)Src[0].epx == (FixMath.Fix64)Src[1].epx && (FixMath.Fix64)Src[0].epy == (FixMath.Fix64)Src[1].epy)
        {
            //tss.GameEndResultSet(true);
            Debug.Log("Same Result");
        }
        else
        {
            //tss.GameEndResultSet(false);
            Debug.LogWarning("Different Result:\n" + "Sent string:" + Src[1].epx + "," + Src[1].epy + "\nReceived string:" + Src[0].epx + "," + Src[0].epy);
        }
        ClearSArray();
        if (RoundNow >= TotalRounds)
        {
            BattlesFinish();
            return;
        }
        else
            RoundNow++;
        MSM.OpenMainSkillMenu();
    }

    public void Sendlsd(SkillData sd)
    {
        Bond.IO.Safe.OutputBuffer ob2 = new Bond.IO.Safe.OutputBuffer(128);
        Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer> boc = new Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob2);
        Serialize.To(boc, sd);
        byte[] sendBytes = new byte[ob2.Data.Array.Length + 1];
        sendBytes[0] = (byte)1;
        ob2.Data.Array.CopyTo(sendBytes, 1);
        for (int i = 0; i < TOmb.Length; i++)
        {
            if (i != clientNum)
                SteamNetworking.SendP2PPacket(TOmb[i], sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
        }
    }

    public void SendQuit()
    {
        byte[] quitBytes = new byte[1];
        quitBytes[0] = 9;
        if (TOmb != null)
        {
            Debug.Log("Total Members Length: " + TOmb.Length);
            foreach (CSteamID i in TOmb)
            {
                SteamNetworking.SendP2PPacket(i, quitBytes, (uint)quitBytes.Length, EP2PSend.k_EP2PSendReliable);
            }
        }
        BattlesFinish();
    }

    public void SendHello(byte hi)
    {
        byte[] hello = new byte[1];
        hello[0] = hi;
        if (TOmb != null)
        {
            foreach (CSteamID i in TOmb)
            {
                SteamNetworking.SendP2PPacket(i, hello, (uint)hello.Length, EP2PSend.k_EP2PSendReliable);
            }
        }
        Debug.Log("Hello Everybody~" + hi);
    }

    public void Send666()
    {
        AudioSource.PlayClipAtPoint(SoundAtStart, GameObject.Find("Main Camera").transform.position);
        EndData endData = new EndData();
        endData.CircleID = 666;
        endData.epx = 0;
        endData.epy = 0;
        started = true;
        SendEnd(endData);
        Debug.Log("666 Sent");
    }

    private void OnApplicationQuit()
    {
        if (enabled)
            SendQuit();
    }

    public void ShowMC()
    {
        MCG.alpha = 1;
        MCG.blocksRaycasts = true;
        MCG.interactable = true;
    }

    public void HideMC()
    {
        MCG.interactable = false;
        MCG.blocksRaycasts = false;
        MCG.alpha = 0;
    }

    private void Update()
    {
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            if (MCG.blocksRaycasts)
                HideMC();
            else
                ShowMC();
        }
        SWControl();
    }

    private void FixedUpdate()
    {
        if (WaitingSkillLevels)
        {
            TimeCount += Time.fixedDeltaTime;
            if (TimeCount >= LearnTime)
            {
                WaitingSkillLevels = false;
                SPNL.alphaset();
                TimeCount = 0;
            }
        }
    }

    public void SWControl()
    {
        //Recieve packets from other members in the lobby with us
        uint msgSize;
        while (SteamNetworking.IsP2PPacketAvailable(out msgSize))
        {
            byte[] packet = new byte[msgSize];
            CSteamID steamIDRemote;
            uint bytesRead = 0;
            if (SteamNetworking.ReadP2PPacket(packet, msgSize, out bytesRead, out steamIDRemote))
            {
                if (Array.IndexOf(TOmb, steamIDRemote) == -1)
                    return;
                int TYPE = packet[0];
                Array.Copy(packet, 1, rcbuffer, 0, packet.Length - 1);
                switch (TYPE)
                {
                    case 0:
                        DeSerializeReceived();
                        break;
                    case 1:
                        SetSD();
                        break;
                    case 2:
                        EndBattle(Array.IndexOf(TOmb, steamIDRemote));
                        break;
                    case 3:
                        testMenu02.ConnectedWho(steamIDRemote);
                        SetCntbAndCheck(Array.IndexOf(TOmb, steamIDRemote));
                        break;
                    case 4:
                        Send666();
                        break;
                    case 5:
                        SetSLTempAndCheck(Array.IndexOf(TOmb, steamIDRemote));
                        break;
                    case 9:
                        heQuit();
                        break;
                    default: Debug.Log("BAD PACKET"); break;
                }
            }
        }
    }

    void heQuit()
    {
        BattlesFinish();
    }

    void SetSD()
    {
        Bond.IO.Safe.InputBuffer ib2 = new Bond.IO.Safe.InputBuffer(rcbuffer);
        Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer> cbr = new Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer>(ib2);
        SkillData CDrc = Deserialize<SkillData>.From(cbr);
        SetTempAndCheck(CDrc.cNum, CDrc.SLs);
    }

    void DeSerializeReceived()
    {
        if (Learning) return;
        MyNS.Eat(rcbuffer);
        rcbuffer = new byte[256];
    }
}

[Serializable, Schema]
public class SkillData
{
    [Id(0)]
    public int cNum;
    [Id(1)]
    public int[] SLs;
}

[Serializable, Schema]
public class ClickData
{
    [Id(0)]
    public MButton? blr;
    [Id(1)]
    public float? xPos;
    [Id(2)]
    public float? yPos;
    [Id(3)]
    public SkillCode? SC;
    [Id(4)]
    public string gn = string.Empty;

    public void Msetdata(MButton b, float? x, float? y)
    {
        blr = b;
        xPos = x;
        yPos = y;
    }
    public void Ssetdata(string n)
    {
        gn = n;
    }
    public void Ksetdata(SkillCode? k)
    {
        SC = k;
    }
}

[Serializable, Schema]
public class Data2S
{
    [Id(0)]
    public int frameNum;
    [Id(1)]
    public int clientNum;
    [Id(2)]
    public List<ClickData> clickDatas;
}

[Serializable, Schema]
public class EndData
{
    [Id(0)]
    public int CircleID;
    [Id(1)]
    public float epx;
    [Id(2)]
    public float epy;
}

public enum GameState { Learning, Fighting };
public enum MButton { left, right, skill };
public enum SkillCode { SkillC1, SkillC2, SkillC3, SkillC4, SkillD2, SkillD3, SkillD4, SkillE1, SkillE1b, SkillE2, SkillE2b, SkillE3, SkillE3b, SkillR1, SkillR1b, SkillR2, SkillR2b, SkillR3b, SkillT1b, SkillT2, SkillT2b, SkillT3, SkillT3b, SkillY1, SkillY1b, SkillY2, SkillY2b, SkillY3, SkillY3b, TestSkill01, TestSkill02, TestSkill03, TestSkillLeech, TestSkillLightning, SelfExplodeScript, FireStop };