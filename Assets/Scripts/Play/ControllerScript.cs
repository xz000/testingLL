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
            Vector2 v2 = new Vector2(CD.xPos, CD.yPos);
            switch (CD.blr)
            {
                case MButton.left:
                    CPCat(PNO, v2);
                    break;
                case MButton.right:
                    theMS[PNO].SetTarget(v2);
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
        /*
        Vector3 v3;
        for (int i = 0; i < MaxNum; i++)
        {
            v3 = new Vector3(i * 3, 0, 0);
            thePC[i] = Instantiate(PlayerCircle, v3, Quaternion.identity);
            theMS[i] = thePC[i].GetComponent<MoveScript>();
        }
        */
    }
}
