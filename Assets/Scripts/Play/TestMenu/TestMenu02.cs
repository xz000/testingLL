using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using Steamworks;
using Bond;

public class TestMenu02 : MonoBehaviour
{
    public Button Startbutton;
    public GameObject Masterpanel;
    public GameObject Otherpanel;
    public Toggle Readytoggle;
    public Toggle AutostartToggle;

    public GameObject SenderPanel;
    public GameObject CVS2;
    public Sender SenderSC;
    public SkillsLink SPNL;
    public UserListScript ULS;

    public Text PlayersJoined;
    //public Text Notready;
    public Text Roomname;
    public InputField MyWord;
    UserWord theword;

    protected Callback<LobbyKicked_t> Callback_LobbyKicked;
    protected Callback<LobbyChatUpdate_t> Callback_LobbyChatUpdate;
    protected Callback<LobbyDataUpdate_t> Callback_LobbyDataUpdate;
    protected Callback<P2PSessionRequest_t> Callback_newConnection;
    protected Callback<LobbyChatMsg_t> Callback_LobbyChatMsg;
    protected Callback<AvatarImageLoaded_t> m_AvatarImageLoaded;

    ///public GameObject Menu00;
    public GameObject Menu01;
    public GameObject DPanel;
    public GameObject MPanel;
    //public CanvasGroup RightGroup;
    Lobby m_CurrentLobby;
    bool hassession;

    //bool isready = false;

    private void Start()
    {
        //Callback_LobbyKicked = Callback<LobbyKicked_t>.Create(OnLobbyKicked);
        Callback_LobbyChatUpdate = Callback<LobbyChatUpdate_t>.Create(OnLobbyChatUpdate);
        Callback_newConnection = Callback<P2PSessionRequest_t>.Create(OnNewConnection);
        Callback_LobbyDataUpdate = Callback<LobbyDataUpdate_t>.Create(OnLobbyDataUpdate);
        Callback_LobbyChatMsg = Callback<LobbyChatMsg_t>.Create(OnLobbyChatMsg);
        m_AvatarImageLoaded = Callback<AvatarImageLoaded_t>.Create(OnAvatarImageLoaded);
    }

    void OnNewConnection(P2PSessionRequest_t result)
    {
        //Debug.Log("Wa");
        if (Array.IndexOf(Sender.TOmb, result.m_steamIDRemote) != -1)
        {
            hassession = SteamNetworking.AcceptP2PSessionWithUser(result.m_steamIDRemote);
            return;
        }
    }

    void OnEnable()
    {
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
        CVS2.SetActive(true);
        Startbutton.gameObject.SetActive(false);
        Readytoggle.interactable = true;
        MPanel.GetComponent<CanvasGroup>().interactable = true;
        UpdateLobbyInfo(ref m_CurrentLobby);
        SetBasic();
    }

    void OnLobbyDataUpdate(LobbyDataUpdate_t lobbyDataUpdate_T)
    {
        Sender.roomid = (CSteamID)lobbyDataUpdate_T.m_ulSteamIDLobby;
        UpdateLobbyInfo(ref m_CurrentLobby);
        SetBasic();
    }

    void OnLobbyChatUpdate(LobbyChatUpdate_t lobbyChatUpdate_T)
    {
        Sender.roomid = (CSteamID)lobbyChatUpdate_T.m_ulSteamIDLobby;
        UpdateLobbyInfo(ref m_CurrentLobby);
        SetBasic();
    }

    void OnAvatarImageLoaded(AvatarImageLoaded_t pCallback)
    {
        UpdateLobbyInfo(ref m_CurrentLobby);
        SetBasic();
    }

    void UpdateLobbyInfo(ref Lobby outLobby)
    {
        outLobby.m_SteamID = Sender.roomid;
        outLobby.m_Owner = SteamMatchmaking.GetLobbyOwner(Sender.roomid);
        outLobby.m_Members = new LobbyMembers[SteamMatchmaking.GetNumLobbyMembers(Sender.roomid)];
        outLobby.m_MemberLimit = SteamMatchmaking.GetLobbyMemberLimit(Sender.roomid);

        int nDataCount = SteamMatchmaking.GetLobbyDataCount(Sender.roomid);
        outLobby.m_Data = new LobbyMetaData[nDataCount];

        int Ucount = SteamMatchmaking.GetNumLobbyMembers(Sender.roomid);
        //ULS.CreateDs(Ucount);
        for (int i = 0; i < nDataCount; ++i)
        {
            bool lobby_data_ret = SteamMatchmaking.GetLobbyDataByIndex(Sender.roomid, i, out outLobby.m_Data[i].m_Key, Constants.k_nMaxLobbyKeyLength, out outLobby.m_Data[i].m_Value, Constants.k_cubChatMetadataMax);
            if (lobby_data_ret) continue;
            Debug.LogError("SteamMatchmaking.GetLobbyDataByIndex returned false.");
            continue;
        }
        //Notready.text = SteamMatchmaking.GetLobbyMemberData(Sender.roomid, SteamUser.GetSteamID(), "key_ready");
        int rc = 0;
        int gc = 0;
        for (int i = 0; i < outLobby.m_Members.Length; i++)
        {
            outLobby.m_Members[i].m_SteamID = SteamMatchmaking.GetLobbyMemberByIndex(Sender.roomid, i);
            //ULS.UDs[i].GetComponent<UserDetailScript>().HomeWork(outLobby.m_Members[i].m_SteamID);
            outLobby.m_Members[i].m_Data = new LobbyMetaData[1];
            LobbyMetaData lmd = new LobbyMetaData();
            lmd.m_Key = "key_ready";
            lmd.m_Value = SteamMatchmaking.GetLobbyMemberData(Sender.roomid, outLobby.m_Members[i].m_SteamID, lmd.m_Key);
            if (lmd.m_Value == "READY")
                rc++;
            if (lmd.m_Value == "GREEN")
                gc++;
            outLobby.m_Members[i].m_Data[0] = lmd;
            //ULS.UDs[i].GetComponent<UserDetailScript>().Uready.text = lmd.m_Value;
            if (outLobby.m_Members[i].m_SteamID == SteamUser.GetSteamID())
                MPControl(lmd.m_Value);
        }
        if (rc == outLobby.m_MemberLimit)
        {
            if (rc == 1)
                TestStart();
            else
                GameStart(rc);
        }
        if (gc == outLobby.m_MemberLimit && SteamMatchmaking.GetLobbyOwner(Sender.roomid) == SteamUser.GetSteamID())
        {
            Debug.Log("All Players Green");
            Startbutton.interactable = true;
        }
    }

    void MPControl(string v)
    {
        if (v == "NOT READY" && !Readytoggle.isOn)
            MPanel.GetComponent<CanvasGroup>().interactable = true;
    }

    public void SetReady()
    {
        if (Readytoggle.isOn)
        {
            if (DataSheetDone())
            {
                MPanel.GetComponent<CanvasGroup>().interactable = false;
                SteamMatchmaking.SetLobbyMemberData(Sender.roomid, "key_ready", "READY");
            }
            else
                Readytoggle.isOn = false;
        }
        else
        {
            SteamMatchmaking.SetLobbyMemberData(Sender.roomid, "key_ready", "NOT READY");
        }
    }

    public void SetGreen()
    {
        SteamMatchmaking.SetLobbyMemberData(Sender.roomid, "key_ready", "GREEN");
    }

    bool DataSheetDone()
    {
        DataShowScript[] dss = GetComponentsInChildren<DataShowScript>(DPanel);
        foreach (DataShowScript d in dss)
        {
            if (d.DataValueText.text == "")
                return false;
        }
        return true;
    }

    public void SetRoundNum(int n)
    {
        SteamMatchmaking.SetLobbyData(Sender.roomid, "Total_Rounds", n.ToString());
    }

    public void SetLearnTime(int n)
    {
        SteamMatchmaking.SetLobbyData(Sender.roomid, "Learn_Time", n.ToString());
    }

    void SetBasic()
    {
        int Mcount = SteamMatchmaking.GetNumLobbyMembers(Sender.roomid);
        ULS.CreateDs(Mcount);
        for (int i = 0; i < Mcount; i++)
        {
            ULS.UDs[i].GetComponent<UserDetailScript>().HomeWork(SteamMatchmaking.GetLobbyMemberByIndex(Sender.roomid, i));
        }
        PlayersJoined.text = Mcount + " players joined";
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
        if (SteamMatchmaking.GetLobbyOwner(Sender.roomid) == SteamUser.GetSteamID())
            MPanel.SetActive(true);
        else
            MPanel.SetActive(false);
        DataShowScript[] dss = GetComponentsInChildren<DataShowScript>(DPanel);
        foreach (DataShowScript d in dss)
            d.GetMyData();
        //if (Mcount == 2) GameStart();
    }

    void GameStart(int r)
    {
        //NetWriter.rs = r;
        //准备SkillCode交错数组
        Readytoggle.interactable = false;
        SenderSC.PrepareTemp(r, (int)SkillCode.SelfExplodeScript);
        //设置clientNum
        CSteamID tid;
        Sender.TOmb = new CSteamID[r];
        for (int i = 0; i < r; i++)
        {
            tid = SteamMatchmaking.GetLobbyMemberByIndex(Sender.roomid, i);
            Sender.TOmb[i] = tid;
            if (tid == SteamUser.GetSteamID())
                Sender.clientNum = i;
        }
        SenderPanel.SetActive(true);
        CVS2.SetActive(true);
        SenderSC.SendHello(3);
        if (SteamMatchmaking.GetLobbyOwner(Sender.roomid) == SteamUser.GetSteamID())
            Startbutton.gameObject.SetActive(true);
    }

    void TestStart()
    {
        Sender.isTesting = true;
        Debug.Log("Now Testing");
        int r = 2;
        //NetWriter.rs = r;
        //准备SkillCode交错数组
        SenderSC.PrepareTemp(r, (int)SkillCode.SelfExplodeScript);
        //设置clientNum
        Sender.TOmb = new CSteamID[r];
        for (int i = 0; i < r; i++)
        {
            Sender.TOmb[i] = SteamUser.GetSteamID();
        }
        Sender.clientNum = 0;
        SenderPanel.SetActive(true);
        CVS2.SetActive(true);
        EndData endData = new EndData();
        endData.CircleID = 666;
        endData.epx = 0;
        endData.epy = 0;
        SenderSC.started = true;
        SenderSC.SendEnd(endData);
        Debug.Log("Test Started");
    }

    public void LeaveLobby()
    {
        GameObject safeground = GameObject.FindGameObjectWithTag("Ground");
        Destroy(safeground);
        GameObject[] pcs = GameObject.FindGameObjectsWithTag("Player");
        foreach (GameObject pc in pcs)
            Destroy(pc);
        if (hassession)
        {
            for (int i = 0; i < Sender.TOmb.Length; i++)
            {
                if (Sender.TOmb[i] != SteamUser.GetSteamID())
                    SteamNetworking.CloseP2PSessionWithUser(Sender.TOmb[i]);
            }
            hassession = false;
        }
        SteamMatchmaking.LeaveLobby(Sender.roomid);
        Readytoggle.isOn = false;
        SwitchToMenu01();
    }

    public void SwitchToMenu01()
    {
        SenderPanel.SetActive(false);
        gameObject.SetActive(false);
        GameObject.Find("Canvas2").SetActive(false);
        //RightGroup.interactable = true;
        Menu01.SetActive(true);
    }

    public void ClickBackButton()
    {
        SenderSC.SendQuit();
    }

    public void SendMyWord()
    {
        Bond.IO.Safe.OutputBuffer outputBuffer = new Bond.IO.Safe.OutputBuffer(4096);
        Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer> compactBinaryWriter = new Bond.Protocols.CompactBinaryWriter<Bond.IO.Safe.OutputBuffer>(outputBuffer);
        UserWord userWord = new UserWord();
        userWord.myWord = MyWord.text;
        MyWord.text = "";
        Serialize.To(compactBinaryWriter, userWord);
        SteamMatchmaking.SendLobbyChatMsg(Sender.roomid, outputBuffer.Data.Array, outputBuffer.Data.Array.Length);
    }

    void OnLobbyChatMsg(LobbyChatMsg_t lobbyChatMsg_T)
    {
        CSteamID Sayer;
        byte[] hua = new byte[4096];
        EChatEntryType ctype;
        if (Sender.roomid == (CSteamID)lobbyChatMsg_T.m_ulSteamIDLobby)
        {
            SteamMatchmaking.GetLobbyChatEntry(Sender.roomid, (int)lobbyChatMsg_T.m_iChatID, out Sayer, hua, hua.Length, out ctype);
            Bond.IO.Safe.InputBuffer inputBuffer = new Bond.IO.Safe.InputBuffer(hua);
            Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer> compactBinaryReader = new Bond.Protocols.CompactBinaryReader<Bond.IO.Safe.InputBuffer>(inputBuffer);
            theword = Deserialize<UserWord>.From(compactBinaryReader);
            foreach (GameObject a in ULS.UDs)
            {
                if (a.GetComponent<UserDetailScript>().ido(Sayer))
                {
                    a.GetComponent<UserDetailScript>().ClassWork(theword.myWord);
                    theword = null;
                    return;
                }
            }
        }
    }

    public void ConnectedWho(CSteamID cSteamID)
    {
        foreach (GameObject a in ULS.UDs)
        {
            if (a.GetComponent<UserDetailScript>().ido(cSteamID))
            {
                a.GetComponent<UserDetailScript>().Uready.text = "Connected";
                a.GetComponent<UserDetailScript>().Uready.color = Color.green;
                return;
            }
        }
    }
}

[Serializable, Schema]
public class UserWord
{
    [Id(0)]
    public string myWord;
}
