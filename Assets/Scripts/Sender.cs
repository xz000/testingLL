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
    public InputField TextToSend;
    public Text TextReceived;
    public HostToggle theHT;
    public ClientToggle theCT;
    public TestSteamworks tss;
    public Image SignalLight;
    public byte[] buffer;
    public int sz;
    public int rcsz;
    public byte error;
    public bool started = false;
    //public static bool isServer;
    public static int clientNum;
    public GameObject SendButton;
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
    public static CSteamID TOmb;
    //public static GameState NowState;
    public static bool Learning = false;
    float TimeCount = 0;
    public static float LearnTime = 5;
    int RoundsPassed = 0;
    public static int TotalRounds = 2;
    EndData sts;
    EndData Src;
    public static int[][] theSLtemp;
    bool[] SLtb;

    private void Start()
    {
        ResetSelf();
        //PrepareTemp(2, (int)SkillCode.SelfExplodeScript);
    }

    public void PrepareTemp(int PlayersCount, int SkillsCount)
    {
        SLtb = new bool[PlayersCount];
        theSLtemp = new int[PlayersCount][];
        for (int i = 0; i < PlayersCount; i++)
            theSLtemp[i] = new int[SkillsCount];
        Debug.Log("交错数组已经建立");
    }

    public void SetTempAndCheck(int cN, int[] cSL)
    {
        for (int i = 0; i < cSL.Length; i++)
            theSLtemp[cN][i] = cSL[i];
        SLtb[cN] = true;
        Debug.Log(cN + " " + SLtb[cN]);
        bool allok = true;
        for (int i = 0; i < SLtb.Length; i++)
            allok = allok & SLtb[i];
        if (allok)
            ConnectDo();
    }

    public void ResetSelf()
    {
        started = false;
        ResetS2();
    }

    void ResetS2()
    {
        Debug.Log("S2");
        //SendButton.SetActive(false);
        SignalLight.color = Color.white;
        MyNS.isstarted = false;
        MyNS.enabled = false;
        CCToggle.isOn = false;
    }

    public void SendEnd(EndData se)
    {
        if (!started)
            return;
        ResetSelf();
        sts = se;
        Bond.IO.Safe.OutputBuffer ob2 = new Bond.IO.Safe.OutputBuffer(128);
        Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer> boc = new Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob2);
        Serialize.To(boc, se);
        ///NetworkTransport.Send(HSID, CNID, CHANID, ob2.Data.Array, ob2.Data.Array.Length, out error);
        byte[] sendBytes = new byte[ob2.Data.Array.Length + 1];
        sendBytes[0] = (byte)2;
        ob2.Data.Array.CopyTo(sendBytes, 1);
        SteamNetworking.SendP2PPacket(TOmb, sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
        EndingCompare();
        Debug.Log("End Sent");
    }

    public void EndBattle()
    {
        Debug.Log("End Received");
        Learning = true;
        //NowState = GameState.Learning;
        Bond.IO.Safe.InputBuffer ib2 = new Bond.IO.Safe.InputBuffer(rcbuffer);
        Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer> cbr = new Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer>(ib2);
        Src = Deserialize<EndData>.From(cbr);
        EndingCompare();
    }

    void EndingCompare()
    {
        if (Src == null || sts == null)
            return;
        Debug.Log("Compareing Ending Place");
        if ((FixMath.Fix64)Src.epx == (FixMath.Fix64)sts.epx && (FixMath.Fix64)Src.epy == (FixMath.Fix64)sts.epy)
        {
            tss.GameEndResultSet(true);
            Debug.Log("Same Result");
        }
        else
        {
            tss.GameEndResultSet(false);
            Debug.Log("Different Result:\n" + "Sent string:" + sts.epx + "," + sts.epy + "\nReceived string:" + Src.epx + "," + Src.epy);
        }
        Src = null;
        sts = null;
        MSM.OpenMainSkillMenu();
        RoundsPassed++;
        if (RoundsPassed >= TotalRounds)
        {
            ShowMC();
            GameObject.Find("RoomPanel").GetComponent<TestMenu02>().ClickBackButton();
        }
    }

    public void ConnectDo()
    {
        //StartSelf();
        started = true;
        SignalLight.color = Color.green;
        //SendButton.SetActive(true);
        MyNS.enabled = true;//开启netwriter
        CCToggle.isOn = true;//开启ClickCatcher
        //SPNL.alphaset();
        HideMC();
    }

    /*void DisconnectDo()
    {
        ResetS2();
        SignalLight.color = Color.red;
        ShowMC();
    }*/

    /*public void StartSelf()
    {
        started = true;
        SignalLight.color = Color.yellow;
    }*/

    public void Sendlsd(SkillData sd)
    {
        Bond.IO.Safe.OutputBuffer ob2 = new Bond.IO.Safe.OutputBuffer(128);
        Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer> boc = new Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob2);
        Serialize.To(boc, sd);
        ///NetworkTransport.Send(HSID, CNID, CHANID, ob2.Data.Array, ob2.Data.Array.Length, out error);
        byte[] sendBytes = new byte[ob2.Data.Array.Length + 1];
        sendBytes[0] = (byte)1;
        ob2.Data.Array.CopyTo(sendBytes, 1);
        SteamNetworking.SendP2PPacket(TOmb, sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
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
        //SteamAPI.RunCallbacks();
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            if (MCG.blocksRaycasts)
                HideMC();
            else
                ShowMC();
        }
        /*if (!started)
            return;*/
        SWControl();
    }

    private void FixedUpdate()
    {
        /*switch (NowState)
        {
            case GameState.Learning:
                TimeCount += Time.fixedDeltaTime;
                if (TimeCount >= LearnTime)
                {
                    SPNL.alphaset();
                    NowState = GameState.Fighting;
                    TimeCount = 0;
                }
                break;
        }*/
        if (Learning)
        {
            TimeCount += Time.fixedDeltaTime;
            if (TimeCount >= LearnTime)
            {
                SPNL.alphaset();
                Learning = false;
                TimeCount = 0;
            }
        }
    }

    public void SWControl()
    {
        //Debug.Log("Checking");
        //Recieve packets from other members in the lobby with us
        uint msgSize;
        while (SteamNetworking.IsP2PPacketAvailable(out msgSize))
        {
            /*
            rcbuffer = new byte[msgSize];
            CSteamID steamIDRemote;
            uint bytesRead = 0;
            if (SteamNetworking.ReadP2PPacket(rcbuffer, msgSize, out bytesRead, out steamIDRemote))
                DeSerializeReceived();
            */
            //Debug.Log(msgSize);
            byte[] packet = new byte[msgSize];
            CSteamID steamIDRemote;
            uint bytesRead = 0;
            if (SteamNetworking.ReadP2PPacket(packet, msgSize, out bytesRead, out steamIDRemote))
            {
                //Debug.Log("Hello");
                int TYPE = packet[0];
                Array.Copy(packet, 1, rcbuffer, 0, packet.Length - 1);
                switch (TYPE)
                {
                    case 0:
                        DeSerializeReceived();
                        break;
                    case 1:
                        //ConnectDo();
                        SetSD();
                        //SPNL.betaset();
                        break;
                    case 2:
                        EndBattle();
                        break;
                    default: Debug.Log("BAD PACKET"); break;
                }
            }
        }
    }

    void SetSD()
    {
        Bond.IO.Safe.InputBuffer ib2 = new Bond.IO.Safe.InputBuffer(rcbuffer);
        Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer> cbr = new Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer>(ib2);
        SkillData CDrc = Deserialize<SkillData>.From(cbr);
        //MyNS.GetComponent<ControllerScript>().SetSkillMem(CDrc.cNum, CDrc.SLs);
        SetTempAndCheck(CDrc.cNum, CDrc.SLs);
    }

    private void PrintReceived()
    {
        TextReceived.text = System.Text.Encoding.Unicode.GetString(rcbuffer);
        rcbuffer = new byte[256];
    }

    void DeSerializeReceived()
    {
        //System.Array.Resize<byte>(ref rcbuffer, rcbfsz);
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