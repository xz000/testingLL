using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class TestMenu02 : MonoBehaviour
{
    public GameObject Startbutton;
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

    protected Callback<LobbyKicked_t> Callback_LobbyKicked;
    protected Callback<LobbyChatUpdate_t> Callback_LobbyChatUpdate;
    protected Callback<LobbyDataUpdate_t> Callback_LobbyDataUpdate;
    protected Callback<P2PSessionRequest_t> Callback_newConnection;
    protected Callback<AvatarImageLoaded_t> m_AvatarImageLoaded;

    ///public GameObject Menu00;
    public GameObject Menu01;
    public GameObject BPanel;
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
        m_AvatarImageLoaded = Callback<AvatarImageLoaded_t>.Create(OnAvatarImageLoaded);
    }

    void OnNewConnection(P2PSessionRequest_t result)
    {
        //Debug.Log("Wa");
        if (Sender.TOmb == result.m_steamIDRemote)
        {
            hassession = SteamNetworking.AcceptP2PSessionWithUser(result.m_steamIDRemote);
            return;
        }
    }

    void OnEnable()
    {
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
        CVS2.SetActive(true);
        SetBasic();
    }

    void OnLobbyDataUpdate(LobbyDataUpdate_t lobbyDataUpdate_T)
    {
        Sender.roomid = (CSteamID)lobbyDataUpdate_T.m_ulSteamIDLobby;
        UpdateLobbyInfo(ref m_CurrentLobby);
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
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
        ULS.CreateDs(Ucount);
        for (int i = 0; i < nDataCount; ++i)
        {
            bool lobby_data_ret = SteamMatchmaking.GetLobbyDataByIndex(Sender.roomid, i, out outLobby.m_Data[i].m_Key, Constants.k_nMaxLobbyKeyLength, out outLobby.m_Data[i].m_Value, Constants.k_cubChatMetadataMax);
            if (lobby_data_ret) continue;
            Debug.LogError("SteamMatchmaking.GetLobbyDataByIndex returned false.");
            continue;
        }
        //Notready.text = SteamMatchmaking.GetLobbyMemberData(Sender.roomid, SteamUser.GetSteamID(), "key_ready");
        int rc = 0;
        for (int i = 0; i < outLobby.m_Members.Length; i++)
        {
            outLobby.m_Members[i].m_SteamID = SteamMatchmaking.GetLobbyMemberByIndex(Sender.roomid, i);
            ULS.UDs[i].GetComponent<UserDetailScript>().HomeWork(outLobby.m_Members[i].m_SteamID);
            outLobby.m_Members[i].m_Data = new LobbyMetaData[1];
            LobbyMetaData lmd = new LobbyMetaData();
            lmd.m_Key = "key_ready";
            lmd.m_Value = SteamMatchmaking.GetLobbyMemberData(Sender.roomid, outLobby.m_Members[i].m_SteamID, lmd.m_Key);
            if (lmd.m_Value == "READY")
                rc++;
            outLobby.m_Members[i].m_Data[0] = lmd;
            ULS.UDs[i].GetComponent<UserDetailScript>().Uready.text = lmd.m_Value;
        }
        if (rc == 2)
            GameStart();
    }

    public void SetReady()
    {
        if (Readytoggle.isOn)
            SteamMatchmaking.SetLobbyMemberData(Sender.roomid, "key_ready", "READY");
        else
            SteamMatchmaking.SetLobbyMemberData(Sender.roomid, "key_ready", "NOT READY");
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
        //if (Mcount == 2) GameStart();
    }

    void GameStart()
    {
        //准备SkillCode交错数组
        SenderSC.PrepareTemp(2, (int)SkillCode.SelfExplodeScript);
        //设置clientNum
        CSteamID tid;
        for (int i = 0; i < 2; i++)
        {
            tid = SteamMatchmaking.GetLobbyMemberByIndex(Sender.roomid, i);
            if (tid != SteamUser.GetSteamID())
                Sender.TOmb = tid;
        }
        if (SteamMatchmaking.GetLobbyOwner(Sender.roomid) == SteamUser.GetSteamID())
        {
            //Sender.isServer = true;
            Sender.clientNum = 0;
        }
        else
        {
            //Sender.isServer = false;
            Sender.clientNum = 1;
        }
        SenderPanel.SetActive(true);
        CVS2.SetActive(true);
        BPanel.SetActive(true);
        EndData endData = new EndData();
        endData.CircleID = 0;
        endData.epx = 0;
        endData.epy = 0;
        SenderSC.started = true;
        SenderSC.SendEnd(endData);
        Debug.Log("enddata sent");
        //RoundStart();
    }

    void RoundStart()
    {
        SPNL.alphaset();
    }

    /*void SayHello()
    {
        byte[] hello = new byte[1];
        hello[0] = 2;
        SteamNetworking.SendP2PPacket(Sender.TOmb, hello, (uint)hello.Length, EP2PSend.k_EP2PSendReliable);
        //SenderSC.ConnectDo();
        Debug.Log("Hi");
        gameObject.SetActive(false);
    }*/

    void LeaveLobby()
    {
        GameObject safeground = GameObject.FindGameObjectWithTag("Ground");
        Destroy(safeground);
        GameObject[] pcs = GameObject.FindGameObjectsWithTag("Player");
        foreach (GameObject pc in pcs)
            Destroy(pc);
        if (hassession)
        {
            SteamNetworking.CloseP2PSessionWithUser(Sender.TOmb);
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
        BPanel.SetActive(false);
        GameObject.Find("Canvas2").SetActive(false);
        //RightGroup.interactable = true;
        Menu01.SetActive(true);
    }

    public void ClickBackButton()
    {
        LeaveLobby();
    }

    public void Updatetext()
    {
        /*PlayersJoined.text = PhotonNetwork.playerList.Length + " players joined";
        int i = 0;
        foreach (PhotonPlayer pp in PhotonNetwork.playerList)
        {
            if ((bool)pp.CustomProperties["ready"] == false)
                i++;
        }
        Notready.text = i + " not ready";
        if (PhotonNetwork.isMasterClient)
        {
            if (i == 0)
            {
                Startbutton.SetActive(true);
                if (AutostartToggle.isOn && PhotonNetwork.playerList.Length > 1)
                    ClickStartButton();
            }
            else
                Startbutton.SetActive(false);
        }*/
    }
}
