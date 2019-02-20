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
    public Sender SenderSC;
    public SkillsLink SPNL;

    public Text PlayersJoined;
    public Text Notready;
    public Text Roomname;

    protected Callback<LobbyKicked_t> Callback_LobbyKicked;
    protected Callback<LobbyChatUpdate_t> Callback_LobbyChatUpdate;
    protected Callback<LobbyDataUpdate_t> Callback_LobbyDataUpdate;
    protected Callback<P2PSessionRequest_t> Callback_newConnection;

    ///public GameObject Menu00;
    public GameObject Menu01;
    public GameObject BPanel;
    public CanvasGroup RightGroup;

    bool isready = false;

    /*public override void OnJoinedRoom()
    {
        setreadystatusonline();
        showpanel();
        Roomname.text = PhotonNetwork.room.Name;
        if (PhotonNetwork.isMasterClient && !PhotonNetwork.room.IsOpen)
            PhotonNetwork.room.IsOpen = true;
    }*/
    void Update()
    {
        SteamAPI.RunCallbacks();
    }
    
    private void Start()
    {
        Callback_LobbyKicked = Callback<LobbyKicked_t>.Create(OnLobbyKicked);
        Callback_LobbyChatUpdate = Callback<LobbyChatUpdate_t>.Create(OnLobbyChatUpdate);
        Callback_LobbyKicked = Callback<LobbyKicked_t>.Create(OnLobbyKicked);
        Callback_newConnection = Callback<P2PSessionRequest_t>.Create(OnNewConnection);
    }

    void OnNewConnection(P2PSessionRequest_t result)
    {
        //Debug.Log("Wa");
        if (Sender.TOmb == result.m_steamIDRemote)
        {
            SteamNetworking.AcceptP2PSessionWithUser(result.m_steamIDRemote);
            return;
        }
    }

    void OnEnable()
    {
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
        SetBasic();
    }

    void OnLobbyKicked(LobbyKicked_t lobbyKicked_T)
    {
        Sender.roomid = (CSteamID)lobbyKicked_T.m_ulSteamIDLobby;
        SwitchToMenu01();
    }

    void OnLobbyDataUpdate(LobbyDataUpdate_t lobbyDataUpdate_T)
    {
        Sender.roomid = (CSteamID)lobbyDataUpdate_T.m_ulSteamIDLobby;
        Roomname.text = SteamMatchmaking.GetLobbyData(Sender.roomid, "name");
    }

    void OnLobbyChatUpdate(LobbyChatUpdate_t lobbyChatUpdate_T)
    {
        Sender.roomid = (CSteamID)lobbyChatUpdate_T.m_ulSteamIDLobby;
        SetBasic();
    }

    void SetBasic()
    {
        int Mcount = SteamMatchmaking.GetNumLobbyMembers(Sender.roomid);
        PlayersJoined.text = Mcount + " players joined";
        if (Mcount == 2)
        {
            CSteamID tid;
            for (int i = 0; i < Mcount; i++)
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
            //SayHello();
            SPNL.alphaset();
            //SPNL.betaset();
            BPanel.SetActive(true);
        }
    }

    void SayHello()
    {
        byte[] hello = new byte[1];
        hello[0] = 2;
        SteamNetworking.SendP2PPacket(Sender.TOmb, hello, (uint)hello.Length, EP2PSend.k_EP2PSendReliable);
        //SenderSC.ConnectDo();
        Debug.Log("Hi");
        gameObject.SetActive(false);
    }

    void LeaveLobby()
    {
        GameObject[] pcs = GameObject.FindGameObjectsWithTag("Player");
        foreach (GameObject pc in pcs)
            Destroy(pc);
        SteamMatchmaking.LeaveLobby(Sender.roomid);
        SwitchToMenu01();
    }

    public void SwitchToMenu01()
    {
        SenderPanel.SetActive(false);
        gameObject.SetActive(false);
        BPanel.SetActive(false);
        Menu01.SetActive(true);
        RightGroup.interactable = true;
    }

    /*public override void OnPhotonPlayerConnected(PhotonPlayer newPlayer)
    {
        Updatetext();
    }

    public override void OnPhotonPlayerDisconnected(PhotonPlayer otherPlayer)
    {
        Updatetext();
    }

    public override void OnMasterClientSwitched(PhotonPlayer newMasterClient)
    {
        showpanel();
    }*/

    public void showmasterpanel()
    {
        Otherpanel.SetActive(false);
        Masterpanel.SetActive(true);
        isready = true;
        setreadystatusonline();
    }

    public void showotherpanel()
    {
        Masterpanel.SetActive(false);
        Otherpanel.SetActive(true);
        isready = Readytoggle.isOn;
        setreadystatusonline();
    }

    public void OnReadyToggleChanged()
    {
        isready = Readytoggle.isOn;
        setreadystatusonline();
    }

    public void setreadystatusonline()
    {
        /*PhotonNetwork.player.SetCustomProperties(new ExitGames.Client.Photon.Hashtable()
        {
        {"ready", isready }
        });
        photonView.RPC("Updatetext", PhotonTargets.All);*/
    }

    public void ClickStartButton()
    {
        ///PhotonNetwork.room.IsOpen = false;
        ///EnterCircleSence();
        //photonView.RPC("EnterCircleSence", PhotonTargets.All);
    }

    public void ClickBackButton()
    {
        LeaveLobby();
    }

    public void showpanel()
    {
        /*if (PhotonNetwork.isMasterClient)
            showmasterpanel();
        else
            showotherpanel();*/
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
