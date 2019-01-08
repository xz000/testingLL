using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class BlueLineScript : MonoBehaviour
{
    public Rigidbody2D sender;
    public Rigidbody2D receiver;
    public LineRenderer MyLine;
    public float speed = 2;
    public Fix64 damage;
    public float maxtime = 2;
    float timepsd = 0;
    //public bool missed;
    //public bool Idrag = false;

	// Use this for initialization
	void Start () {
        timepsd = 0;
    }
	
	// Update is called once per frame
	void Update ()
    {
        drawmyline(sender.position, receiver.position);
    }

    void FixedUpdate()
    {
        timepsd += Time.fixedDeltaTime;
        receiver.GetComponent<HPScript>().GetHurt(damage * (Fix64)Time.fixedDeltaTime);
        if (timepsd >= maxtime || receiver == null || sender == null)
            gameObject.GetComponent<DestroyScript>().Destroyself();
    }

    public void SetBSC(float spd, Fix64 dmg, float maxT)
    {
        damage = dmg;
        speed = spd;
        maxtime = maxT;
    }

    public void BlueLineWorking(Rigidbody2D victimId)
    {
        receiver = victimId;
        receiver.GetComponent<MoveScript>().cook += AddConstentCentrallyVelocity;
        receiver.GetComponent<DoSkill>().ClearDebuff += gameObject.GetComponent<DestroyScript>().Destroyself;
        enabled = true;
    }

    public void BlueLineMissed(Vector2 missedplace)
    {
        drawmyline(sender.position, missedplace);
        StartCoroutine(EraseLine());
    }

    public void AddConstentCentrallyVelocity(Rigidbody2D victim, MoveScript worker)
    {
        Vector2 distance = sender.position - victim.position;
        if (distance.sqrMagnitude > 1.1)
            worker.VelotoAdd += distance.normalized * speed;
    }

    void OnDestroy()
    {
        if (receiver != null)
        {
            receiver.GetComponent<MoveScript>().cook -= AddConstentCentrallyVelocity;
            receiver.GetComponent<DoSkill>().ClearDebuff -= gameObject.GetComponent<DestroyScript>().Destroyself;
        }
    }
            
    void drawmyline(Vector2 v21,Vector2 v22)
    {
        MyLine.SetPosition(0, v21);
        MyLine.SetPosition(1, v22);
    }

    IEnumerator EraseLine()
    {
        yield return new WaitForSeconds(0.1f);
        gameObject.GetComponent<DestroyScript>().Destroyself();
    }
}
