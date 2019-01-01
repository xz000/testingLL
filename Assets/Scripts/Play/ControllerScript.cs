using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class ControllerScript : MonoBehaviour
{
    public GameObject PlayerCircle;
    GameObject[] thePC;
    MoveScript[] theMS;

    public void powerrr(int PNO, List<ClickData> LCD)
    {
        foreach(ClickData CD in LCD)
        {
            switch (CD.blr)
            {
                case MButton.left:
                    Vector2 v2l = new Vector2((float)CD.xPos, (float)CD.yPos);
                    //CPCat(PNO, v2l);
                    break;
                case MButton.right:
                    Vector2 v2r = new Vector2((float)CD.xPos, (float)CD.yPos);
                    theMS[PNO].SetTarget(v2r);
                    break;
            }
        }
        LCD.Clear();
    }

    public void CPCat(int Num, Vector2 place)
    {
        thePC[Num] = Instantiate(PlayerCircle, place, Quaternion.identity);
        theMS[Num] = thePC[Num].GetComponent<MoveScript>();
        if (Num == Sender.clientNum)
            theMS[Num].itsme();
    }

    public void createPCs(int MaxNum)
    {
        thePC = new GameObject[MaxNum];
        theMS = new MoveScript[MaxNum];
        PCBorn(MaxNum);
    }

    void PCBorn(int MNum)
    {
        Vector3 v3;
        for (int i = 0; i < MNum; i++)
        {
            v3 = new Vector3(i * 3, 0, 0);
            thePC[i] = Instantiate(PlayerCircle, v3, Quaternion.identity);
            theMS[i] = thePC[i].GetComponent<MoveScript>();
            if (i == Sender.clientNum)
                theMS[i].itsme();
        }
    }
}
